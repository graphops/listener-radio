use anyhow::anyhow;
use graphcast_sdk::graphcast_agent::waku_handling::network_check;
use graphcast_sdk::WakuMessage;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::time::{interval, sleep, timeout};
use tracing::{debug, info, trace, warn};

use graphcast_sdk::graphcast_agent::{
    message_typing::GraphcastMessage, waku_handling::connected_peer_count, GraphcastAgent,
};

use crate::db::resolver::retain_max_storage;
use crate::metrics::{CONNECTED_PEERS, GOSSIP_PEERS, RECEIVED_MESSAGES, PRUNED_MESSAGES};
use crate::{
    config::Config,
    db::resolver::{add_message, list_messages},
    message_types::{PublicPoiMessage, SimpleMessage, UpgradeIntentMessage},
    metrics::{handle_serve_metrics, ACTIVE_PEERS, CACHED_MESSAGES},
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
}

impl RadioOperator {
    /// Create a radio operator with radio configurations, persisted data,
    /// graphcast agent, and control flow
    pub async fn new(config: Config, graphcast_agent: GraphcastAgent) -> RadioOperator {
        debug!("Set global static instance of graphcast_agent");
        let graphcast_agent = Arc::new(graphcast_agent);
        _ = GRAPHCAST_AGENT.set(graphcast_agent.clone());

        let notifier = Notifier::from_config(&config);

        debug!("Connecting to database");

        let db = PgPoolOptions::new()
            .max_connections(50)
            .acquire_timeout(Duration::from_secs(3))
            .connect(&config.database_url)
            .await
            .expect("Could not connect to DATABASE_URL");

        debug!("Check for database migration");
        sqlx::migrate!()
            .run(&db)
            .await
            .expect("Could not run migration");

        debug!("Initialized Radio Operator");
        RadioOperator {
            config,
            db,
            graphcast_agent,
            notifier,
        }
    }

    /// Preparation for running the radio applications
    /// Expose metrics and subscribe to graphcast topics
    pub async fn prepare(&self, receiver: Receiver<WakuMessage>) {
        // Set up Prometheus metrics url if configured
        if let Some(port) = self.config.metrics_port {
            debug!("Initializing metrics port");
            tokio::spawn(handle_serve_metrics(self.config.metrics_host.clone(), port));
        }

        if let Some(true) = self.config.filter_protocol {
            // Provide generated topics to Graphcast agent
            let topics = self.config.topics.to_vec();
            debug!(
                topics = tracing::field::debug(&topics),
                "Found content topics for subscription",
            );
            self.graphcast_agent
                .update_content_topics(topics.clone())
                .await;
        }

        message_processor(self.db.clone(), receiver).await;
        GRAPHCAST_AGENT
            .get()
            .unwrap()
            .register_handler()
            .expect("Could not register handler");
    }

    pub fn graphcast_agent(&self) -> &GraphcastAgent {
        &self.graphcast_agent
    }

    /// Radio operations
    pub async fn run(&self) {
        // Control flow
        // TODO: expose to radio config for the users
        let running = Arc::new(AtomicBool::new(true));
        let skip_iteration = Arc::new(AtomicBool::new(false));
        let skip_iteration_clone = skip_iteration.clone();

        let mut network_update_interval = interval(Duration::from_secs(600));
        let mut comparison_interval = interval(Duration::from_secs(30));

        let iteration_timeout = Duration::from_secs(180);
        let update_timeout = Duration::from_secs(5);

        // Separate thread to skip a main loop iteration when hit timeout
        tokio::spawn(async move {
            tokio::time::sleep(iteration_timeout).await;
            skip_iteration_clone.store(true, Ordering::SeqCst);
        });

        // Initialize Http server with graceful shutdown if configured
        if self.config.server_port().is_some() {
            let config = self.config.clone();
            let db = self.db.clone();
            tokio::spawn(run_server(config, db, running.clone()));
        }

        // Main loop for sending messages, can factor out
        // and take radio specific query and parsing for radioPayload
        while running.load(Ordering::SeqCst) {
            if self.graphcast_agent.number_of_peers() == 0 {
                info!("No active peers on the network, sleep for 10 seconds");
                let _ = sleep(Duration::from_secs(10)).await;
            }
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
                        if skip_iteration.load(Ordering::SeqCst) {
                            skip_iteration.store(false, Ordering::SeqCst);
                            continue;
                        }
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
                    if skip_iteration.load(Ordering::SeqCst) {
                        skip_iteration.store(false, Ordering::SeqCst);
                        continue;
                    }

                    // Prune old messages
                    let num_pruned = 
                        match timeout(update_timeout,
                            retain_max_storage(&self.db, self.config.max_storage.try_into().unwrap())
                        ).await {
                            Err(e) => {debug!(err = tracing::field::debug(e), "Pruning time out"); 0},
                            Ok(Ok(num_pruned)) => {
                                PRUNED_MESSAGES.set(num_pruned);
                                num_pruned
                            }
                            Ok(Err(e)) => {
                                warn!(err = tracing::field::debug(e), "Pruning time out"); 0
                            }
                        };

                    // List the ones leftover
                    let result = timeout(update_timeout,
                        list_messages::<GraphcastMessage<PublicPoiMessage>>(&self.db)
                    ).await;

                    match result {
                        Err(e) => warn!(err = tracing::field::debug(e), "Public PoI messages summary timed out"),
                        Ok(msgs) => {
                            let msg_num = &msgs.map_or(0, |m| m.len());
                            CACHED_MESSAGES.set(*msg_num as i64);
                            info!(total_messages = msg_num,
                                num_pruned,
                                "Monitoring summary"
                            )
                        }
                    }
                },
                else => break,
            }

            sleep(Duration::from_secs(5)).await;
            continue;
        }
    }
}

pub async fn message_processor(db_ref: Pool<Postgres>, receiver: Receiver<WakuMessage>) {
    thread::spawn(move || {
        let rt = Runtime::new().expect("Could not create Tokio runtime");
        let db_ref_rt = db_ref.clone();
        for msg in receiver {
            rt.block_on(async {
                trace!("Message processing");
                RECEIVED_MESSAGES.inc();
                let timeout_duration = Duration::from_secs(1);
                let process_res = timeout(timeout_duration, process_message(&db_ref_rt, msg)).await;
                match process_res {
                    Ok(Ok(r)) => trace!(msg_row_id = r, "New message added to DB"),
                    Ok(Err(e)) => {
                        trace!(err = tracing::field::debug(&e), "Failed to process message");
                    }
                    Err(e) => debug!(error = e.to_string(), "Message processor timed out"),
                }
            });
        }
    });
}

pub async fn process_message(db: &Pool<Postgres>, msg: WakuMessage) -> Result<i64, anyhow::Error> {
    if let Ok(msg) = GraphcastMessage::<PublicPoiMessage>::decode(msg.payload()).await {
        add_message(db, msg).await
    } else if let Ok(msg) = GraphcastMessage::<UpgradeIntentMessage>::decode(msg.payload()).await {
        add_message(db, msg).await
    } else if let Ok(msg) = GraphcastMessage::<SimpleMessage>::decode(msg.payload()).await {
        add_message(db, msg).await
    } else {
        trace!(
            topic = tracing::field::debug(msg.content_topic()),
            "Message decode failed"
        );
        Err(anyhow!("Unsupported message types"))
    }
}
