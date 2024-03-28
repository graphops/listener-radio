use anyhow::anyhow;
use chrono::Utc;
use graphcast_sdk::WakuMessage;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::time::{interval, sleep, timeout};
use tracing::{debug, info, trace, warn};

use graphcast_sdk::graphcast_agent::{message_typing::GraphcastMessage, GraphcastAgent};

use crate::db::resolver::{
    count_messages, get_indexer_stats, insert_aggregate, prune_old_messages, retain_max_storage,
};
use crate::metrics::{CONNECTED_PEERS, GOSSIP_PEERS, PRUNED_MESSAGES, RECEIVED_MESSAGES};
use crate::{
    config::Config,
    db::resolver::add_message,
    message_types::{PublicPoiMessage, SimpleMessage, UpgradeIntentMessage},
    metrics::{handle_serve_metrics, ACTIVE_PEERS, CACHED_MESSAGES},
    server::run_server,
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
    running: Arc<AtomicBool>,
    message_processor_handle: JoinHandle<()>,
}

impl RadioOperator {
    /// Create a radio operator with radio configurations, persisted data,
    /// graphcast agent, and control flow
    pub async fn new(
        config: Config,
        graphcast_agent: GraphcastAgent,
        receiver: Receiver<WakuMessage>,
    ) -> RadioOperator {
        let running = Arc::new(AtomicBool::new(true));

        debug!("Set global static instance of graphcast_agent");
        let graphcast_agent = Arc::new(graphcast_agent);
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

        // Set up Prometheus metrics url if configured
        if let Some(port) = config.metrics_port {
            debug!("Initializing metrics port");
            tokio::spawn(handle_serve_metrics(config.metrics_host.clone(), port));
        }

        if let Some(true) = config.filter_protocol {
            // Provide generated topics to Graphcast agent
            let topics = config.topics.to_vec();
            debug!(
                topics = tracing::field::debug(&topics),
                "Found content topics for subscription",
            );
            graphcast_agent.update_content_topics(topics.clone());
        }

        let message_processor_handle = message_processor(db.clone(), receiver).await;
        debug!("Initialized Radio Operator");
        RadioOperator {
            config,
            db,
            graphcast_agent,
            notifier,
            running,
            message_processor_handle,
        }
    }

    pub fn graphcast_agent(&self) -> &GraphcastAgent {
        &self.graphcast_agent
    }

    /// Radio operations
    pub async fn run(&self) {
        // Control flow
        let running = self.running.clone();
        let skip_iteration = Arc::new(AtomicBool::new(false));
        let skip_iteration_clone = skip_iteration.clone();

        let mut network_update_interval = interval(Duration::from_secs(600));
        let mut summary_interval = interval(Duration::from_secs(180));
        let mut daily_aggregate_interval = interval(Duration::from_secs(86400)); // 24 hours

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
                    let connection = self.graphcast_agent.network_check();
                    debug!(network_check = tracing::field::debug(&connection), "Network condition");

                    // Update the number of peers connected
                    let connected_peers = self.graphcast_agent.connected_peer_count().unwrap_or_default() as i64;
                    CONNECTED_PEERS.set(connected_peers);
                    GOSSIP_PEERS.set(self.graphcast_agent.number_of_peers().try_into().unwrap_or_default());

                    if let Some(true) = self.config.filter_protocol {
                        if skip_iteration.load(Ordering::SeqCst) {
                            skip_iteration.store(false, Ordering::SeqCst);
                            continue;
                        }

                        // Update topic subscription
                        self.graphcast_agent()
                            .update_content_topics(self.config.topics.to_vec());

                        ACTIVE_PEERS
                            .set(self.graphcast_agent.number_of_peers().try_into().unwrap());
                    }
                },
                _ = summary_interval.tick() => {
                    trace!("Local summary update");
                    if skip_iteration.load(Ordering::SeqCst) {
                        skip_iteration.store(false, Ordering::SeqCst);
                        continue;
                    }

                    let mut total_num_pruned: i64 = 0;

                    // Conditionally prune based on max_storage if provided
                    if let Some(max_storage) = self.config.max_storage {
                        let max_storage_usize = max_storage as usize;
                        match timeout(
                            update_timeout,
                            retain_max_storage(&self.db, max_storage_usize)
                        ).await {
                            Err(e) => debug!(err = tracing::field::debug(e), "Pruning by max storage timed out"),
                            Ok(Ok(num_pruned)) => {
                                total_num_pruned += num_pruned;
                                PRUNED_MESSAGES.set(total_num_pruned);
                            },
                            Ok(Err(e)) => warn!(err = tracing::field::debug(e), "Error during pruning by max storage"),
                        };
                    }

                    let batch_size = 1000;

                    // Always prune old messages based on RETENTION
                    match timeout(
                        update_timeout,
                        prune_old_messages(&self.db, self.config.retention, batch_size)
                    ).await {
                        Err(e) => debug!(err = tracing::field::debug(e), "Pruning by retention timed out"),
                        Ok(Ok(num_pruned)) => {
                            total_num_pruned += num_pruned;
                            PRUNED_MESSAGES.set(total_num_pruned);
                        },
                        Ok(Err(e)) => warn!(err = tracing::field::debug(e), "Error during pruning by retention"),
                    };

                    // List the remaining messages
                    let result = timeout(update_timeout, count_messages(&self.db)).await.expect("could not count messages");

                    match result {
                        Err(e) => warn!(err = tracing::field::debug(e), "Database query for message count timed out"),
                        Ok(count) => {
                            CACHED_MESSAGES.set(count);
                            info!(total_messages = count,
                                  total_num_pruned,
                                  "Monitoring summary"
                            )
                        }
                    }
                },
                _ = daily_aggregate_interval.tick() => {
                    if skip_iteration.load(Ordering::SeqCst) {
                        skip_iteration.store(false, Ordering::SeqCst);
                        continue;
                    }

                    let pool = &self.db;
                    let from_timestamp = (Utc::now() - Duration::from_secs(86400)).timestamp();

                    match get_indexer_stats(pool, None, from_timestamp).await {
                        Ok(stats) => {
                            for stat in stats {
                                match insert_aggregate(pool, Utc::now().timestamp(), stat.graph_account, stat.message_count, stat.subgraphs_count).await {
                                    Ok(_) => warn!("Successfully inserted daily aggregate."),
                                    Err(e) => warn!("Failed to insert daily aggregate: {:?}", e),
                                }
                            }
                        },
                        Err(e) => warn!("Failed to fetch indexer stats: {:?}", e),
                    }
                },

                else => break,
            }

            sleep(Duration::from_secs(5)).await;
            continue;
        }
    }
}

pub async fn message_processor(
    db_ref: Pool<Postgres>,
    receiver: Receiver<WakuMessage>,
) -> JoinHandle<()> {
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
    })
}

pub async fn process_message(db: &Pool<Postgres>, msg: WakuMessage) -> Result<i64, anyhow::Error> {
    if let Ok(msg) = GraphcastMessage::<PublicPoiMessage>::decode(msg.payload()) {
        add_message(db, msg).await
    } else if let Ok(msg) = GraphcastMessage::<UpgradeIntentMessage>::decode(msg.payload()) {
        add_message(db, msg).await
    } else if let Ok(msg) = GraphcastMessage::<SimpleMessage>::decode(msg.payload()) {
        add_message(db, msg).await
    } else {
        trace!(
            topic = tracing::field::debug(msg.content_topic()),
            "Message decode failed"
        );
        Err(anyhow!("Unsupported message types"))
    }
}
