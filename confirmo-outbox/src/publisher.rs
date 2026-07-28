use std::time::Duration;

use rdkafka::message::{Header, OwnedHeaders};
use rdkafka::producer::{FutureProducer, FutureRecord};

use crate::error::OutboxError;
use crate::repository::OutboxMessageDAO;

const TRACE_ID_HEADER: &str = "x-trace-id";
const SEND_TIMEOUT: Duration = Duration::from_secs(5);

#[mockall::automock]
#[async_trait::async_trait]
pub trait MessagePublisher: Send + Sync + 'static {
    async fn publish(&self, message: &OutboxMessageDAO) -> Result<(), OutboxError>;
}

pub struct KafkaMessagePublisher {
    producer: FutureProducer,
}

impl KafkaMessagePublisher {
    pub fn new(producer: FutureProducer) -> Self {
        Self { producer }
    }
}

#[async_trait::async_trait]
impl MessagePublisher for KafkaMessagePublisher {
    async fn publish(&self, message: &OutboxMessageDAO) -> Result<(), OutboxError> {
        let payload = serde_json::to_vec(&message.payload)?;
        let key = message.partition_key.as_deref().unwrap_or_default();

        let mut record = FutureRecord::to(&message.topic).payload(&payload).key(key);
        if let Some(trace_id) = &message.trace_id {
            record = record.headers(OwnedHeaders::new().insert(Header {
                key: TRACE_ID_HEADER,
                value: Some(trace_id),
            }));
        }

        self.producer
            .send(record, SEND_TIMEOUT)
            .await
            .map_err(|(e, _)| OutboxError::Kafka(e))?;

        Ok(())
    }
}
