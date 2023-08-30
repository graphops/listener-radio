use dotenv::dotenv;
use graphcast_sdk::{graphcast_agent::GraphcastAgent, WakuMessage};
use listener_radio::{config::Config, operator::RadioOperator};
use std::sync::mpsc;

#[tokio::main]
async fn main() {
    // console_subscriber::init();
    dotenv().ok();

    // Parse basic configurations
    let radio_config = Config::args();
    let (sender, receiver) = mpsc::channel::<WakuMessage>();
    // Initialization
    let agent = GraphcastAgent::new(
        radio_config.to_graphcast_agent_config().await.unwrap(),
        sender,
    )
    .await
    .expect("Initialize Graphcast agent");

    let radio_operator = RadioOperator::new(radio_config, agent).await;

    // Start separate processes
    radio_operator.prepare(receiver).await;

    // Start radio operations
    radio_operator.run().await;
}
