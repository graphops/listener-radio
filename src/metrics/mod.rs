use autometrics::{encode_global_metrics, global_metrics_exporter};
use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use once_cell::sync::Lazy;
use prometheus::{core::Collector, Registry};
use prometheus::{IntCounterVec, IntGauge, IntCounter, Opts};
use std::{net::SocketAddr, str::FromStr};
use tracing::{debug, info};

/// Received (and validated) messages counter
#[allow(dead_code)]
pub static VALIDATED_MESSAGES: Lazy<IntCounterVec> = Lazy::new(|| {
    let m = IntCounterVec::new(
        Opts::new("validated_messages", "Number of validated messages")
            .namespace("graphcast")
            .subsystem("listener_radio"),
        &["deployment"],
    )
    .expect("Failed to create validated_messages counters");
    prometheus::register(Box::new(m.clone()))
        .expect("Failed to register validated_messages counters");
    m
});

/// Received invalid messages counter
#[allow(dead_code)]
pub static INVALIDATED_MESSAGES: Lazy<IntCounterVec> = Lazy::new(|| {
    let m = IntCounterVec::new(
        Opts::new("invalid_messages", "Number of invalid messages received")
            .namespace("graphcast")
            .subsystem("listener_radio"),
        &["error_type"],
    )
    .expect("Failed to create invalid_messages counters");
    prometheus::register(Box::new(m.clone()))
        .expect("Failed to register invalid_messages counters");
    m
});

/// Received (and validated) messages counter
#[allow(dead_code)]
pub static CACHED_MESSAGES: Lazy<IntGauge> = Lazy::new(|| {
    let m = IntGauge::with_opts(
        Opts::new("cached_messages", "Number of messages in cache")
            .namespace("graphcast")
            .subsystem("listener_radio"),
    )
    .expect("Failed to create cached_messages gauges");
    prometheus::register(Box::new(m.clone())).expect("Failed to register cached_messages guage");
    m
});

/// Number of active peers discoverable by listener-radio
/// Updated periodically for the recently received messages
#[allow(dead_code)]
pub static ACTIVE_PEERS: Lazy<IntGauge> = Lazy::new(|| {
    let m = IntGauge::with_opts(
        Opts::new(
            "active_peers",
            "Number of discoverable active peers on network",
        )
        .namespace("graphcast")
        .subsystem("listener_radio"),
    )
    .expect("Failed to create active_peers gauges");
    prometheus::register(Box::new(m.clone())).expect("Failed to register active_peers guage");
    m
});

#[allow(dead_code)]
pub static CONNECTED_PEERS: Lazy<IntGauge> = Lazy::new(|| {
    let m = IntGauge::with_opts(
        Opts::new(
            "connected_peers",
            "Number of Gossip peers connected with Graphcast agent",
        )
        .namespace("graphcast")
        .subsystem("listener_radio"),
    )
    .expect("Failed to create connected_peers gauge");
    prometheus::register(Box::new(m.clone())).expect("Failed to register connected_peers gauge");
    m
});

#[allow(dead_code)]
pub static GOSSIP_PEERS: Lazy<IntGauge> = Lazy::new(|| {
    let m = IntGauge::with_opts(
        Opts::new("gossip_peers", "Total number of gossip peers discovered")
            .namespace("graphcast")
            .subsystem("listener_radio"),
    )
    .expect("Failed to create gossip_peers gauge");
    prometheus::register(Box::new(m.clone())).expect("Failed to register gossip_peers gauge");
    m
});

#[allow(dead_code)]
pub static RECEIVED_MESSAGES: Lazy<IntCounter> = Lazy::new(|| {
    let m = IntCounter::with_opts(
        Opts::new("received_messages", "Number of messages received in total")
            .namespace("graphcast")
            .subsystem("listener_radio"),
    )
    .expect("Failed to create received_messages counter");
    prometheus::register(Box::new(m.clone()))
        .expect("Failed to register received_messages counter");
    m
});

#[allow(dead_code)]
pub static REGISTRY: Lazy<prometheus::Registry> = Lazy::new(prometheus::Registry::new);

#[allow(dead_code)]
pub fn register_metrics(registry: &Registry, metrics: Vec<Box<dyn Collector>>) {
    for metric in metrics {
        registry.register(metric).expect("Cannot register metrics");
        debug!("registered metric");
    }
}

#[allow(dead_code)]
pub fn start_metrics() {
    register_metrics(
        &REGISTRY,
        vec![
            Box::new(VALIDATED_MESSAGES.clone()),
            Box::new(INVALIDATED_MESSAGES.clone()),
            Box::new(CACHED_MESSAGES.clone()),
            Box::new(ACTIVE_PEERS.clone()),
            Box::new(CONNECTED_PEERS.clone()),
            Box::new(GOSSIP_PEERS.clone()),
            Box::new(RECEIVED_MESSAGES.clone()),
        ],
    );
}

/// This handler serializes the metrics into a string for Prometheus to scrape
#[allow(dead_code)]
pub async fn get_metrics() -> (StatusCode, String) {
    match encode_global_metrics() {
        Ok(metrics) => (StatusCode::OK, metrics),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{err:?}")),
    }
}

/// Run the API server as well as Prometheus and a traffic generator
#[allow(dead_code)]
pub async fn handle_serve_metrics(host: String, port: u16) {
    // Set up the exporter to collect metrics
    let _exporter = global_metrics_exporter();

    let app = Router::new().route("/metrics", get(get_metrics));
    let addr =
        SocketAddr::from_str(&format!("{}:{}", host, port)).expect("Start Prometheus metrics");
    let server = axum::Server::bind(&addr);
    info!(
        address = addr.to_string(),
        "Prometheus Metrics port exposed"
    );

    server
        .serve(app.into_make_service())
        .await
        .expect("Error starting example API server");
}
