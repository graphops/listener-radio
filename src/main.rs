use dotenv::dotenv;
use ethers::{
    providers::{Http, Middleware, Provider},
    types::U64,
};
use graphcast_sdk::{
    gossip_agent::{message_typing::GraphcastMessage, GossipAgent},
    init_tracing,
};
use once_cell::sync::OnceCell;
use std::env;
use std::sync::{Arc, Mutex};
use std::{thread::sleep, time::Duration};
use tokio::sync::Mutex as AsyncMutex;
use tracing::{debug, error, info};
use radio_types::RadioPayloadMessage;

mod radio_types;

#[tokio::main]
async fn main() {
    dotenv().ok();
    init_tracing();

    //TODO: Use backend storage instead
    pub static MESSAGES: OnceCell<Arc<Mutex<Vec<GraphcastMessage<RadioPayloadMessage>>>>> =
        OnceCell::new();
    pub static GOSSIP_AGENT: OnceCell<GossipAgent> = OnceCell::new();

    let private_key = env::var("PRIVATE_KEY").expect("No operator private key provided.");
    let eth_node = env::var("ETH_NODE").expect("No ETH URL provided.");
    let provider: Provider<Http> = Provider::<Http>::try_from(eth_node.clone()).unwrap();
    let waku_host = env::var("WAKU_HOST").ok();
    let waku_port = env::var("WAKU_PORT").ok();
    let registry_subgraph = env::var("REGISTRY_SUBGRAPH").expect("No registry subgraph endpoint provided.");
    let network_subgraph = env::var("NETWORK_SUBGRAPH").expect("No network subgraph endpoint provided.");
    let radio_name: &str = "3la";

    let gossip_agent = GossipAgent::new(
        private_key,
        eth_node,
        radio_name, 
        None,
        &registry_subgraph,
        &network_subgraph,
        waku_host,
        waku_port,
        None,
    )
    .await
    .unwrap();

    _ = GOSSIP_AGENT.set(gossip_agent);
    _ = MESSAGES.set(Arc::new(Mutex::new(vec![])));

    let radio_handler =
        |msg: Result<GraphcastMessage<RadioPayloadMessage>, anyhow::Error>| match msg {
            Ok(msg) => {
                //TODO: Send messages to backend storage
                MESSAGES.get().unwrap().lock().unwrap().push(msg);
            }
            Err(err) => {
                println!("{}", err);
            }
        };

    GOSSIP_AGENT
        .get()
        .unwrap()
        .register_handler(Arc::new(Mutex::new(radio_handler)));

    loop {
        let block_number = U64::as_u64(&provider.get_block_number().await.unwrap());
        //TODO: Add statistics about the stored MESSAGES
        debug!("🔗 Block number: {}\nIn-memory messages: {:#?}", block_number, MESSAGES.get().unwrap());

        // Wait before next block check
        sleep(Duration::from_secs(10));
    }
}
