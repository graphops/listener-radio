use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use ethers::providers::{Provider, Http, Middleware};
use tokio::sync::Mutex as AsyncMutex;
use num_bigint::BigUint;
use once_cell::sync::OnceCell;
use waku::Signal;
use anyhow::anyhow;
use colored::*;
use ethers_contract::EthAbiType;
use ethers_core::types::transaction::eip712::Eip712;
use ethers_derive_eip712::*;
use prost::Message;
use serde::{Deserialize, Serialize};
use tracing::{error, info, debug};

use radio_types::RadioPayloadMessage;
use graphcast_sdk::gossip_agent::{
    message_typing::{get_indexer_stake, GraphcastMessage, self},
    GossipAgent, AgentError,
};

mod radio_types;

/// A global static (singleton) instance of GossipAgent. It is useful to ensure that we have only one GossipAgent
/// per Radio instance, so that we can keep track of state and more easily test our Radio application.
pub static GOSSIP_AGENT: OnceCell<GossipAgent> = OnceCell::new();

///TODO: Save the messages to a local store
/// No processing later, but should use a storage backend
pub fn message_handler() -> impl Fn(Result<GraphcastMessage<RadioPayloadMessage>, anyhow::Error>)
{
    info!("what's happening? ");
    |msg: Result<GraphcastMessage<RadioPayloadMessage>, anyhow::Error>| match msg {
        Ok(msg) => {
            info!("graphcast message: {:#?}", msg)
        }
        Err(err) => {
            error!("{}", err);
        }
    }
}
