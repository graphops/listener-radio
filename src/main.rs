use dotenv::dotenv;
use graphcast_sdk::{graphcast_agent::GraphcastAgent, WakuMessage};
use listener_radio::{config::Config, operator::RadioOperator};
use tracing::debug;
use std::sync::mpsc;

#[tokio::main]
async fn main() {
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

    // let token = CancellationToken::new();
    let radio_operator = RadioOperator::new(radio_config, agent).await;

    // Set up message processor after receving message from Graphcast agent
    let process_handler = radio_operator.message_processor(receiver).await;
    debug!(h = tracing::field::debug(&process_handler), "process handle");
    radio_operator.add_handler(process_handler).await;
    
    // Start radio operations
    let main_loop_handler = radio_operator.run().await;
    debug!(h = tracing::field::debug(&main_loop_handler), "main handle");
    radio_operator.add_handler(main_loop_handler).await;
}
