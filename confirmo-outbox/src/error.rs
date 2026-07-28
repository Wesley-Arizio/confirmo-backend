use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutboxError {
    #[error("outbox database error")]
    Database(#[from] sqlx::Error),

    #[error("failed to serialize outbox payload")]
    Serialize(#[from] serde_json::Error),

    #[error("failed to publish outbox message to kafka")]
    Kafka(#[source] rdkafka::error::KafkaError),
}
