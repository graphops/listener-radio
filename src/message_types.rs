use async_graphql::SimpleObject;
use ethers_contract::EthAbiType;
use ethers_core::types::transaction::eip712::Eip712;
use ethers_derive_eip712::*;
use prost::Message;
use serde::{Deserialize, Serialize};

#[derive(Eip712, EthAbiType, Clone, Message, Serialize, Deserialize, PartialEq, SimpleObject)]
#[eip712(
    name = "PublicPoiMessage",
    version = "0",
    chain_id = 1,
    verifying_contract = "0xc944e90c64b2c07662a292be6244bdf05cda44a7"
)]
pub struct PublicPoiMessage {
    #[prost(string, tag = "1")]
    pub identifier: String,
    #[prost(string, tag = "2")]
    pub content: String,
    //TODO: see if timestamp that comes with waku message can be used
    /// nonce cached to check against the next incoming message
    #[prost(int64, tag = "3")]
    pub nonce: i64,
    /// blockchain relevant to the message
    #[prost(string, tag = "4")]
    pub network: String,
    /// block relevant to the message
    #[prost(uint64, tag = "5")]
    pub block_number: u64,
    /// block hash generated from the block number
    #[prost(string, tag = "6")]
    pub block_hash: String,
    /// Graph account sender
    #[prost(string, tag = "7")]
    pub graph_account: String,
}

/// Make a test radio type
#[derive(Eip712, EthAbiType, Clone, Message, Serialize, Deserialize, SimpleObject)]
#[eip712(
    name = "Graphcast Ping-Pong Radio",
    version = "0",
    chain_id = 1,
    verifying_contract = "0xc944e90c64b2c07662a292be6244bdf05cda44a7"
)]
pub struct SimpleMessage {
    #[prost(string, tag = "1")]
    pub identifier: String,
    #[prost(string, tag = "2")]
    pub content: String,
}

#[derive(Eip712, EthAbiType, Clone, Message, Serialize, Deserialize, PartialEq, SimpleObject)]
#[eip712(
    name = "VersionUpgradeMessage",
    version = "0",
    chain_id = 1,
    verifying_contract = "0xc944e90c64b2c07662a292be6244bdf05cda44a7"
)]
pub struct VersionUpgradeMessage {
    // identify through the current subgraph deployment
    #[prost(string, tag = "1")]
    pub identifier: String,
    // new version of the subgraph has a new deployment hash
    #[prost(string, tag = "2")]
    pub new_hash: String,
    /// subgraph id shared by both versions of the subgraph deployment
    #[prost(string, tag = "6")]
    pub subgraph_id: String,
    /// nonce cached to check against the next incoming message
    #[prost(int64, tag = "3")]
    pub nonce: i64,
    /// blockchain relevant to the message
    #[prost(string, tag = "4")]
    pub network: String,
    /// estimated timestamp for the usage to switch to the new version
    #[prost(int64, tag = "5")]
    pub migrate_time: i64,
    /// Graph account sender - expect the sender to be subgraph owner
    #[prost(string, tag = "7")]
    pub graph_account: String,
}
