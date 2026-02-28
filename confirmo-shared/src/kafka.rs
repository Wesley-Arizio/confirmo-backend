use std::sync::Arc;

use rdkafka::{
    Message,
    consumer::{CommitMode, Consumer},
    message::{BorrowedMessage, Headers},
};
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::constants::TRACE_ID_HEADER;

pub struct KafkaMessageProcessor<C> {
    consumer: Arc<C>,
}

pub fn get_trace_id(message: &BorrowedMessage<'_>) -> String {
    if let Some(headers) = message.headers() {
        for i in 0..headers.count() {
            let header = headers.get(i);
            if header.key == TRACE_ID_HEADER {
                if let Some(value) = header.value {
                    return String::from_utf8_lossy(value).to_string();
                }
            }
        }
    }

    tracing::info!("Tracing ID not found in headers, using default.");
    Uuid::new_v4().to_string()
}

impl<C> KafkaMessageProcessor<C>
where
    C: Consumer,
{
    pub fn new(consumer: Arc<C>) -> Self {
        Self { consumer }
    }

    pub async fn process<F, Fut, E>(
        &self,
        message: &BorrowedMessage<'_>,
        trace_id: &str,
        event_name: &'static str,
        handler: F,
    ) where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<(), E>>,
        E: std::fmt::Debug,
    {
        let span = info_span!("kafka.message", event = event_name, trace_id = trace_id);

        async {
            tracing::info!("Started Processing");
            match handler().await {
                Ok(()) => {
                    tracing::info!("Processing succeeded");
                    if let Err(e) = self.consumer.commit_message(&message, CommitMode::Async) {
                        tracing::error!(
                            error = ?e,
                            "Failed to commit Kafka message"
                        );
                    }
                }
                Err(e) => {
                    tracing::error!("Processing Kafka Message Failed: {:?}", e);
                }
            }
        }
        .instrument(span)
        .await;
    }
}
