use async_graphql::SimpleObject;
use ethers_contract::EthAbiType;
use ethers_core::types::transaction::eip712::Eip712;
use ethers_derive_eip712::*;
use graphcast_sdk::graphcast_agent::message_typing::{
    GraphcastMessage, MessageError, RadioPayload,
};
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
    #[prost(uint64, tag = "3")]
    pub nonce: u64,
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

impl RadioPayload for PublicPoiMessage {
    /// Check duplicated fields: payload message has duplicated fields with GraphcastMessage, the values must be the same
    fn valid_outer(&self, outer: &GraphcastMessage<Self>) -> Result<&Self, MessageError> {
        if self.nonce == outer.nonce
            && self.graph_account == outer.graph_account
            && self.identifier == outer.identifier
        {
            Ok(self)
        } else {
            Err(MessageError::InvalidFields(anyhow::anyhow!(
                "Radio message wrapped by inconsistent GraphcastMessage: {:#?} <- {:#?}\nnonce check: {:#?}\naccount check: {:#?}\nidentifier check: {:#?}",
                &self,
                &outer,
                self.nonce == outer.nonce,
                self.graph_account == outer.graph_account,
                self.identifier == outer.identifier,
            )))
        }
    }
}

/// Make a test radio type
#[derive(Eip712, EthAbiType, Clone, Message, Serialize, Deserialize, SimpleObject)]
#[eip712(
    name = "PingPongMessage",
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

impl RadioPayload for SimpleMessage {
    fn valid_outer(&self, outer: &GraphcastMessage<Self>) -> Result<&Self, MessageError> {
        if self.identifier == outer.identifier {
            Ok(self)
        } else {
            Err(MessageError::InvalidFields(anyhow::anyhow!(
                "Radio message wrapped by inconsistent GraphcastMessage: {:#?} <- {:#?}",
                &self,
                &outer,
            )))
        }
    }
}

#[derive(Eip712, EthAbiType, Clone, Message, Serialize, Deserialize, PartialEq, SimpleObject)]
#[eip712(
    name = "UpgradeIntentMessage",
    version = "0",
    chain_id = 1,
    verifying_contract = "0xc944e90c64b2c07662a292be6244bdf05cda44a7"
)]
pub struct UpgradeIntentMessage {
    /// current subgraph deployment hash
    #[prost(string, tag = "1")]
    pub deployment: String,
    /// subgraph id shared by both versions of the subgraph deployment
    #[prost(string, tag = "2")]
    pub subgraph_id: String,
    // new version of the subgraph has a new deployment hash
    #[prost(string, tag = "3")]
    pub new_hash: String,
    /// nonce cached to check against the next incoming message
    #[prost(uint64, tag = "4")]
    pub nonce: u64,
    /// Graph account sender - expect the sender to be subgraph owner
    #[prost(string, tag = "5")]
    pub graph_account: String,
}

impl RadioPayload for UpgradeIntentMessage {
    /// Check duplicated fields: payload message has duplicated fields with GraphcastMessage, the values must be the same
    fn valid_outer(&self, outer: &GraphcastMessage<Self>) -> Result<&Self, MessageError> {
        if self.nonce == outer.nonce && self.graph_account == outer.graph_account {
            Ok(self)
        } else {
            Err(MessageError::InvalidFields(anyhow::anyhow!(
                "Radio message wrapped by inconsistent GraphcastMessage: {:#?} <- {:#?}",
                &self,
                &outer,
            )))
        }
    }
}
