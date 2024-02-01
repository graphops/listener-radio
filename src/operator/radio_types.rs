use async_graphql::SimpleObject;
use ethers_contract::EthAbiType;
use ethers_core::types::transaction::eip712::Eip712;
use ethers_derive_eip712::*;
use graphcast_sdk::graphcast_agent::message_typing::{
    GraphcastMessage, MessageError, RadioPayload,
};
use prost::Message;
use serde::{Deserialize, Serialize};

// Specifically support a list of radios for listener-radio to instruct how to read the radio message types
// and derive eip712 types to support signature encoding and decoding of the payload message.
// EIP-712 name will change encoding, append other unique message types/names to decode properly
#[derive(Eip712, EthAbiType, Clone, Message, Serialize, Deserialize, PartialEq, SimpleObject)]
#[eip712(
    name = "RadioPayloadMessage",
    version = "0",
    chain_id = 1,
    verifying_contract = "0xc944e90c64b2c07662a292be6244bdf05cda44a7"
)]
pub struct RadioPayloadMessage {
    #[prost(string, tag = "1")]
    pub identifier: String,
    #[prost(string, tag = "2")]
    pub content: String,
}

impl RadioPayloadMessage {
    pub fn new(identifier: String, content: String) -> Self {
        RadioPayloadMessage {
            identifier,
            content,
        }
    }

    pub fn payload_content(&self) -> String {
        self.content.clone()
    }
}

impl RadioPayload for RadioPayloadMessage {
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
