use autometrics::autometrics;
use std::sync::{mpsc, Mutex as SyncMutex};
use tracing::{error, trace};
use graphcast_sdk::graphcast_agent::{
    message_typing::GraphcastMessage, waku_handling::WakuHandlingError,
};

use crate::{metrics::VALIDATED_MESSAGES, operator::RadioOperator};

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
            // TODO: Handle the error case by incrementing a Prometheus "error" counter
            if let Ok(msg) = msg {
                trace!(msg = tracing::field::debug(&msg), "Received message");
                let id: String = msg.identifier.clone();
                VALIDATED_MESSAGES.with_label_values(&[&id]).inc();
                match sender.lock().unwrap().send(msg) {
                    //TODO: Compare if the message should be stored here or when it gets to the receiver
                    //Probably the receiver as the handler is limited
                    Ok(_) => trace!("Sent received message to radio operator"),
                    Err(e) => error!("Could not send message to channel, {:#?}", e),
                };

                //TODO: Make sure CACHED_MESSAGES is updated
            } else {
                trace!(msg = tracing::field::debug(&msg), "Invalid message");
            }
        }
    }
}
