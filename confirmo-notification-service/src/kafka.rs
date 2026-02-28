use std::sync::Arc;

use rdkafka::{
    consumer::{CommitMode, Consumer},
    message::BorrowedMessage,
};
use tracing::{Instrument, info_span};

pub struct KafkaMessageProcessor<C> {
    consumer: Arc<C>,
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
