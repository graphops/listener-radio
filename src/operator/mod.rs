use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex as SyncMutex};
use std::thread;
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::{interval, sleep, timeout};
use tracing::{debug, info, trace, warn};

use graphcast_sdk::{
    build_wallet,
    graphcast_agent::{message_typing::GraphcastMessage, GraphcastAgent},
    graphcast_id_address,
    graphql::client_registry::query_registry_indexer,
};

use crate::config::Config;
use crate::db::resolver::{add_message, list_messages};
use crate::metrics::handle_serve_metrics;
use crate::operator::radio_types::RadioPayloadMessage;
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
    indexer_address: String,
}

impl RadioOperator {
    /// Create a radio operator with radio configurations, persisted data,
    /// graphcast agent, and control flow
    pub async fn new(config: Config) -> RadioOperator {
        debug!("Initializing Radio operator");
        let wallet = build_wallet(
            config
                .wallet_input()
                .expect("Operator wallet input invalid"),
        )
        .expect("Radio operator cannot build wallet");
        // The query here must be Ok but so it is okay to panic here
        // Alternatively, make validate_set_up return wallet, address, and stake
        let indexer_address = query_registry_indexer(
            config.registry_subgraph.to_string(),
            graphcast_id_address(&wallet),
        )
        .await
        .expect("Radio operator registered to indexer");

        debug!("Initializing Graphcast Agent");
        let graphcast_agent = Arc::new(
            config
                .create_graphcast_agent()
                .await
                .expect("Initialize Graphcast agent"),
        );
        debug!("Set global static instance of graphcast_agent");
        _ = GRAPHCAST_AGENT.set(graphcast_agent.clone());

        let notifier = Notifier::from_config(&config);

        debug!("Connecting to database");

        let db = PgPoolOptions::new()
            .max_connections(50)
            .connect_timeout(Duration::from_secs(3))
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
            indexer_address,
        }
    }

    /// Preparation for running the radio applications
    /// Expose metrics and subscribe to graphcast topics
    pub async fn prepare(&self) {
        // Set up Prometheus metrics url if configured
        if let Some(port) = self.config.metrics_port {
            debug!("Initializing metrics port");
            tokio::spawn(handle_serve_metrics(self.config.metrics_host.clone(), port));
        }

        if let Some(true) = self.config.filter_protocol {
            // Provide generated topics to Graphcast agent
            let topics = self
                .config
                .generate_topics(self.indexer_address.clone())
                .await;
            debug!(
                topics = tracing::field::debug(&topics),
                "Found content topics for subscription",
            );
            self.graphcast_agent
                .update_content_topics(topics.clone())
                .await;
        }

        let (sender, receiver) = mpsc::channel::<GraphcastMessage<RadioPayloadMessage>>();
        let handler = RadioOperator::radio_msg_handler(SyncMutex::new(sender));
        GRAPHCAST_AGENT
            .get()
            .unwrap()
            .register_handler(Arc::new(AsyncMutex::new(handler)))
            .expect("Could not register handler");

        let db = self.db.clone();
        thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                for msg in receiver {
                    info!(
                        "Radio operator received a validated message from Graphcast agent: {:#?}",
                        msg
                    );
                    if let Err(e) = add_message(&db, msg).await {
                        warn!(err = tracing::field::debug(&e), "Failed to store message");
                    } else {
                        let msgs =
                            list_messages::<GraphcastMessage<RadioPayloadMessage>>(&db).await;
                        trace!(msgs = tracing::field::debug(&msgs), "now there is!");
                    };
                }
            })
        });
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

        let mut topic_update_interval = interval(Duration::from_secs(600));
        let mut comparison_interval = interval(Duration::from_secs(30));

        let iteration_timeout = Duration::from_secs(180);
        let update_timeout = Duration::from_secs(5);

        // Separate thread to skip a main loop iteration when hit timeout
        tokio::spawn(async move {
            tokio::time::sleep(iteration_timeout).await;
            skip_iteration_clone.store(true, Ordering::SeqCst);
        });

        // Main loop for sending messages, can factor out
        // and take radio specific query and parsing for radioPayload
        while running.load(Ordering::SeqCst) {
            // Run event intervals sequentially by satisfication of other intervals and corresponding tick
            tokio::select! {
                _ = topic_update_interval.tick() => {
                    if let Some(true) = self.config.filter_protocol {
                        if skip_iteration.load(Ordering::SeqCst) {
                            skip_iteration.store(false, Ordering::SeqCst);
                            continue;
                        }
                        // Update topic subscription
                        let result = timeout(update_timeout,
                            self.graphcast_agent()
                            .update_content_topics(self.config.generate_topics(self.indexer_address.clone()).await)
                        ).await;

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
                    // TODO: Generate some summary
                },
                else => break,
            }

            sleep(Duration::from_secs(5)).await;
            continue;
        }
    }
}
