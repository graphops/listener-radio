use async_graphql::{Error, ErrorExtensions};
use autometrics::autometrics;
use once_cell::sync::OnceCell;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::signal;
use tracing::error;

use graphcast_sdk::{
    graphcast_agent::GraphcastAgent,
    graphcast_agent::GraphcastAgentError,
    graphql::client_network::query_network_subgraph,
    graphql::{client_graph_node::get_indexing_statuses, QueryError},
    networks::NetworkName,
    BlockPointer,
};

pub mod config;
pub mod db;
pub mod message_types;
pub mod metrics;
pub mod operator;
pub mod server;

/// A global static (singleton) instance of GraphcastAgent. It is useful to ensure that we have only one GraphcastAgent
/// per Radio instance, so that we can keep track of state and more easily test our Radio application.
pub static GRAPHCAST_AGENT: OnceCell<Arc<GraphcastAgent>> = OnceCell::new();

pub fn radio_name() -> &'static str {
    "graphcast-3la"
}

/// Generate default topics that is operator address resolved to indexer address
/// and then its active on-chain allocations -> function signature should just return
/// A vec of strings for subtopics
pub async fn active_allocation_hashes(
    network_subgraph: &str,
    indexer_address: &str,
) -> Vec<String> {
    query_network_subgraph(network_subgraph, indexer_address)
        .await
        .map(|result| result.indexer_allocations())
        .unwrap_or_else(|e| {
            error!(err = tracing::field::debug(&e), "Failed to generate topics");
            vec![]
        })
}

/// Generate content topics for all deployments that are syncing on Graph node
/// filtering for deployments on an index node
pub async fn syncing_deployment_hashes(
    graph_node_endpoint: &str,
    // graphQL filter
) -> Vec<String> {
    get_indexing_statuses(graph_node_endpoint)
        .await
        .map_err(|e| -> Vec<String> {
            error!(err = tracing::field::debug(&e), "Topic generation error");
            [].to_vec()
        })
        .unwrap()
        .iter()
        .filter(|&status| status.node.is_some() && status.node != Some(String::from("removed")))
        .map(|s| s.subgraph.clone())
        .collect::<Vec<String>>()
}

/// This function returns the string representation of a set of network mapped to their chainhead blocks
#[autometrics]
pub fn chainhead_block_str(
    network_chainhead_blocks: &HashMap<NetworkName, BlockPointer>,
) -> String {
    let mut blocks_str = String::new();
    blocks_str.push_str("{ ");
    for (i, (network, block_pointer)) in network_chainhead_blocks.iter().enumerate() {
        if i > 0 {
            blocks_str.push_str(", ");
        }
        blocks_str.push_str(&format!("{}: {}", network, block_pointer.number));
    }
    blocks_str.push_str(" }");
    blocks_str
}

/// Graceful shutdown when receive signal
pub async fn shutdown_signal(running_program: Arc<AtomicBool>) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {println!("Shutting down server...");},
        _ = terminate => {},
    }

    running_program.store(false, Ordering::SeqCst);
    opentelemetry::global::shutdown_tracer_provider();
}

#[derive(Debug, thiserror::Error)]
pub enum OperationError {
    #[error("Send message trigger isn't met: {0}")]
    SendTrigger(String),
    #[error("Message sent already, skip to avoid duplicates: {0}")]
    SkipDuplicate(String),
    #[error("Comparison trigger isn't met: {0}")]
    CompareTrigger(String, u64, String),
    #[error("Agent encountered problems: {0}")]
    Agent(GraphcastAgentError),
    #[error("Failed to query: {0}")]
    Query(QueryError),
    #[error("Others: {0}")]
    Others(String),
}

impl OperationError {
    pub fn clone_with_inner(&self) -> Self {
        match self {
            OperationError::SendTrigger(msg) => OperationError::SendTrigger(msg.clone()),
            OperationError::SkipDuplicate(msg) => OperationError::SkipDuplicate(msg.clone()),
            OperationError::CompareTrigger(d, b, m) => {
                OperationError::CompareTrigger(d.clone(), *b, m.clone())
            }
            e => OperationError::Others(e.to_string()),
        }
    }
}

impl ErrorExtensions for OperationError {
    fn extend(&self) -> Error {
        Error::new(format!("{}", self))
    }
}
