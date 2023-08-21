use anyhow::anyhow;
use graphcast_sdk::graphcast_agent::waku_handling::network_check;
use graphcast_sdk::WakuMessage;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, sleep, timeout};
use tracing::{debug, info, trace, warn};

use graphcast_sdk::graphcast_agent::{message_typing::GraphcastMessage, GraphcastAgent};

use crate::config::Config;
use crate::db::resolver::{add_message, list_messages};
use crate::message_types::{PublicPoiMessage, SimpleMessage, UpgradeIntentMessage};
use crate::metrics::{handle_serve_metrics, ACTIVE_PEERS, CACHED_MESSAGES};
use crate::operator::radio_types::RadioPayloadMessage;
use crate::server::run_server;
use crate::GRAPHCAST_AGENT;

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

        self.message_processor(receiver).await;
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
            // Run event intervals sequentially by satisfication of other intervals and corresponding tick
            tokio::select! {
                _ = network_update_interval.tick() => {
                    let connection = network_check(&self.graphcast_agent().node_handle);
                    debug!(network_check = tracing::field::debug(&connection), "Network condition");

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
                    if skip_iteration.load(Ordering::SeqCst) {
                        skip_iteration.store(false, Ordering::SeqCst);
                        continue;
                    }
                    let msgs =
                        list_messages::<GraphcastMessage<RadioPayloadMessage>>(&self.db).await;
                    let msg_num = &msgs.map_or(0, |m| m.len());
                    CACHED_MESSAGES
                        .set(*msg_num as i64);
                    info!(total_messages = msg_num,
                        total_topic_filters = self.graphcast_agent.content_identifiers().await.len(),
                        "Monitoring summary"
                    );
                },
                else => break,
            }

            sleep(Duration::from_secs(5)).await;
            continue;
        }
    }

    pub async fn message_processor(&self, receiver: Receiver<WakuMessage>) {
        let agent_ref = self.graphcast_agent.clone();
        let db_ref = self.db.clone();
        tokio::spawn(async move {
            for msg in receiver {
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
        });
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
