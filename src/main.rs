use dotenv::dotenv;
use graphcast_3la::{config::Config, operator::RadioOperator};

#[tokio::main]
async fn main() {
    dotenv().ok();

    // Parse basic configurations
    let radio_config = Config::args();

    // Initialization
    let radio_operator = RadioOperator::new(radio_config).await;

    // Start separate processes
    radio_operator.prepare().await;

    // Start radio operations
    radio_operator.run().await;
}
