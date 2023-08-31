use anyhow::anyhow;
use graphcast_sdk::graphcast_agent::waku_handling::network_check;
use graphcast_sdk::WakuMessage;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};

use tokio::task::JoinHandle;


use std::sync::atomic::{AtomicBool};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::sync::Mutex as SyncMutex;
use std::time::Duration;
use tokio::time::{interval, sleep, timeout};
use tracing::{debug, info, trace, warn};

use graphcast_sdk::graphcast_agent::{
    message_typing::GraphcastMessage, waku_handling::connected_peer_count, GraphcastAgent,
};

use crate::metrics::{CONNECTED_PEERS, GOSSIP_PEERS, RECEIVED_MESSAGES};
use crate::{
    config::Config,
    db::resolver::{add_message, list_messages},
    message_types::{PublicPoiMessage, SimpleMessage, UpgradeIntentMessage},
    metrics::{handle_serve_metrics, ACTIVE_PEERS, CACHED_MESSAGES},
    operator::radio_types::RadioPayloadMessage,
    server::run_server,
    GRAPHCAST_AGENT,
};

use self::notifier::Notifier;

pub mod notifier;
pub mod operation;
pub mod radio_types;

/// Radio operator contains all states needed for radio operations
#[allow(unused)]
pub struct RadioOperator {
    config: Config,
    db: Pool<Postgres>,
    graphcast_agent: Arc<GraphcastAgent>,
    notifier: Notifier,
    thread_handlers: Arc<SyncMutex<Vec<JoinHandle<()>>>>,
}

impl RadioOperator {
    /// Create a radio operator with radio configurations, persisted data,
    /// graphcast agent, and control flow
    // pub async fn new(config: Config, graphcast_agent: GraphcastAgent, token: CancellationToken) -> RadioOperator {
    pub async fn new(config: Config, graphcast_agent: GraphcastAgent) -> RadioOperator {
        debug!("Create a join handle tracker");

        let thread_handlers: Arc<SyncMutex<Vec<tokio::task::JoinHandle<()>>>> =
            Arc::new(SyncMutex::new(Vec::new()));

        debug!("Set global static instance of graphcast_agent");
        let graphcast_agent = Arc::new(graphcast_agent);
        _ = GRAPHCAST_AGENT.set(graphcast_agent.clone());
        let agent_runtime = GRAPHCAST_AGENT
            .get()
            .unwrap()
            .register_handler()
            .expect("Could not register handler");

        let notifier = Notifier::from_config(&config);

        // Set up Prometheus metrics url if configured
        if let Some(port) = config.metrics_port {
            debug!("Initializing metrics port");
            
            let metrics_host = config.metrics_host().clone();
            let handle = tokio::spawn(handle_serve_metrics(metrics_host, port));
            // tokio::select! {
            //     // Step 3: Using cloned token to listen to cancellation requests
            //     _ = cloned_token.cancelled() => {
            //         // The token was cancelled, task can shut down
            //     }
            //     _ = tokio::time::sleep(std::time::Duration::from_secs(9999)) => {
            //         // Long work has completed
            //     }
            // }
            debug!(h = tracing::field::debug(&handle), "metrics handle");
            thread_handlers.lock().unwrap().push(handle);
        };

        debug!("Connecting to database");
        let db = PgPoolOptions::new()
            .max_connections(50)
            .acquire_timeout(Duration::from_secs(3))
            .connect(config.database_url())
            .await
            .expect("Could not connect to DATABASE_URL");

        // Initialize Http server with graceful shutdown if configured
        if config.server_port().is_some() {
            let config = config.clone();
            let db = db.clone();
            let handle = tokio::spawn(run_server(config, db, thread_handlers.clone(), agent_runtime));
            debug!(h = tracing::field::debug(&handle), "server handle");
            thread_handlers.lock().unwrap().push(handle);
        }

        debug!("Check for database migration");
        sqlx::migrate!()
            .run(&db)
            .await
            .expect("Could not run migration");

        if let Some(true) = config.filter_protocol {
            // Provide generated topics to Graphcast agent
            let topics = config.topics.to_vec();
            debug!(
                topics = tracing::field::debug(&topics),
                "Found content topics for subscription",
            );
            graphcast_agent.update_content_topics(topics.clone()).await;
        }

        debug!("Initialized Radio Operator");
        RadioOperator {
            config,
            db,
            graphcast_agent,
            notifier,
            thread_handlers,
        }
    }

    pub fn graphcast_agent(&self) -> &GraphcastAgent {
        &self.graphcast_agent
    }

    /// Radio operations
    pub async fn run(&self) -> JoinHandle<()>{
        // Control flow
        // TODO: expose to radio config for the users
        let _skip_iteration = Arc::new(AtomicBool::new(false));

        let mut network_update_interval = interval(Duration::from_secs(600));
        let mut comparison_interval = interval(Duration::from_secs(30));

        let _iteration_timeout = Duration::from_secs(180);
        let update_timeout = Duration::from_secs(5);

        // Main loop for sending messages, can factor out
        // and take radio specific query and parsing for radioPayload
        loop {
            // Run event intervals sequentially by satisfication of other intervals and corresponding tick
            tokio::select! {
                _ = network_update_interval.tick() => {
                    trace!("Network update");
                    let connection = network_check(&self.graphcast_agent().node_handle);
                    debug!(network_check = tracing::field::debug(&connection), "Network condition");
                    // Update the number of peers connected
                    CONNECTED_PEERS.set(connected_peer_count(&self.graphcast_agent().node_handle).unwrap_or_default().try_into().unwrap_or_default());
                    GOSSIP_PEERS.set(self.graphcast_agent.number_of_peers().try_into().unwrap_or_default());

                    if let Some(true) = self.config.filter_protocol {
                        // Update topic subscription
                        let result = timeout(update_timeout,
                            self.graphcast_agent()
                            .update_content_topics(self.config.topics.to_vec())
                        ).await;

                        ACTIVE_PEERS
                            .set(self.graphcast_agent.number_of_peers().try_into().unwrap());

                        if result.is_err() {
                            warn!("update_content_topics timed out");
                        } else {
                            debug!("update_content_topics completed");
                        }
                    }
                },
                _ = comparison_interval.tick() => {
                    trace!("Local summary update");
                    let result = timeout(update_timeout,
                        list_messages::<GraphcastMessage<RadioPayloadMessage>>(&self.db)
                    ).await;

                    match result {
                        Err(e) => warn!(err = tracing::field::debug(e), "Summary timed out"),
                        Ok(msgs) => {
                            let msg_num = &msgs.map_or(0, |m| m.len());
                            CACHED_MESSAGES.set(*msg_num as i64);
                            info!(total_messages = msg_num,
                                "Monitoring summary"
                            )
                        }
                    }
                },
                else => continue,
            }

            sleep(Duration::from_secs(5)).await;
        }
    }

    pub async fn message_processor(&self, receiver: Receiver<WakuMessage>) -> JoinHandle<()> {
        let agent_ref = self.graphcast_agent.clone();
        let db_ref = self.db.clone();
        tokio::spawn(async move {
            for msg in receiver {
                trace!("Message processing");
                RECEIVED_MESSAGES.inc();
                let timeout_duration = Duration::from_secs(1);
                let process_res =
                    timeout(timeout_duration, process_message(&agent_ref, &db_ref, msg)).await;
                match process_res {
                    Ok(Ok(r)) => trace!(msg_row_id = r, "New message added to DB"),
                    Ok(Err(e)) => {
                        warn!(err = tracing::field::debug(&e), "Failed to process message");
                    }
                    Err(e) => debug!(error = e.to_string(), "Message processor timed out"),
                }
            }
        })
    }

    pub async fn add_handler(&self, handle: JoinHandle<()>) {
        self.thread_handlers.lock().unwrap().push(handle);
    }
}

pub async fn process_message(
    graphcast_agent: &Arc<GraphcastAgent>,
    db: &Pool<Postgres>,
    msg: WakuMessage,
) -> Result<i64, anyhow::Error> {
    if let Ok(msg) = graphcast_agent
        .decode::<PublicPoiMessage>(msg.payload())
        .await
    {
        add_message(db, msg).await
    } else if let Ok(msg) = graphcast_agent
        .decode::<UpgradeIntentMessage>(msg.payload())
        .await
    {
        add_message(db, msg).await
    } else if let Ok(msg) = graphcast_agent.decode::<SimpleMessage>(msg.payload()).await {
        add_message(db, msg).await
    } else {
        Err(anyhow!("Message cannot be decoded"))
    }
}
