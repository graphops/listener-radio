use autometrics::autometrics;
use graphcast_sdk::graphcast_agent::{
    message_typing::GraphcastMessage, waku_handling::WakuHandlingError,
};
use std::sync::{mpsc, Mutex as SyncMutex};
use tracing::{error, trace};

use crate::{
    metrics::{INVALIDATED_MESSAGES, VALIDATED_MESSAGES},
    operator::RadioOperator,
};

use super::radio_types::RadioPayloadMessage;

impl RadioOperator {
    /// Custom callback for handling the validated GraphcastMessage, in this case we only save the messages to a local store
    /// to process them at a later time. This is required because for the processing we use async operations which are not allowed
    /// in the handler.
    #[autometrics]
    pub fn radio_msg_handler(
        sender: SyncMutex<mpsc::Sender<GraphcastMessage<RadioPayloadMessage>>>,
    ) -> impl Fn(Result<GraphcastMessage<RadioPayloadMessage>, WakuHandlingError>) {
        move |msg: Result<GraphcastMessage<RadioPayloadMessage>, WakuHandlingError>| {
            match msg {
                Ok(msg) => {
                    trace!(msg = tracing::field::debug(&msg), "Received message");
                    let id: String = msg.identifier.clone();
                    VALIDATED_MESSAGES.with_label_values(&[&id]).inc();
                    match sender.lock().unwrap().send(msg) {
                        Ok(_) => trace!("Sent received message to radio operator"),
                        Err(e) => error!("Could not send message to channel, {:#?}", e),
                    };
                }
                Err(e) => {
                    INVALIDATED_MESSAGES.with_label_values(&[e.type_string()]).inc();
                    trace!(msg = tracing::field::debug(&e), "Invalid message");
                }
            }
        }
    }
}
