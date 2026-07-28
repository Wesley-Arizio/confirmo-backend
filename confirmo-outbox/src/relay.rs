use std::sync::Arc;
use std::time::Duration;

use crate::error::OutboxError;
use crate::publisher::MessagePublisher;
use crate::repository::OutboxRepository;

const BATCH_SIZE: i64 = 100;
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Background worker that drains the outbox to Kafka. Delivery is at-least-once:
/// a crash after publishing but before marking a row re-sends it on restart, so
/// consumers must be idempotent.
pub struct OutboxRelay {
    repository: Arc<dyn OutboxRepository>,
    publisher: Arc<dyn MessagePublisher>,
}

impl OutboxRelay {
    pub fn new(
        repository: Arc<dyn OutboxRepository>,
        publisher: Arc<dyn MessagePublisher>,
    ) -> Self {
        Self {
            repository,
            publisher,
        }
    }

    /// Poll the outbox, publish pending rows in order, mark them sent. Runs forever.
    pub async fn run(self) {
        loop {
            if let Err(e) = self.publish_pending().await {
                tracing::error!("Outbox relay error: {:?}", e);
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    async fn publish_pending(&self) -> Result<(), OutboxError> {
        for message in self.repository.unpublished(BATCH_SIZE).await? {
            // Mark only after a successful publish. If publish fails we stop and
            // leave the row unpublished, so it is retried on the next poll.
            self.publisher.publish(&message).await?;
            self.repository.mark_published(message.id).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publisher::MockMessagePublisher;
    use crate::repository::{MockOutboxRepository, OutboxMessageDAO};
    use mockall::predicate::eq;
    use rdkafka::error::KafkaError;
    use serde_json::Value;
    use uuid::Uuid;

    fn message(id: Uuid) -> OutboxMessageDAO {
        OutboxMessageDAO {
            id,
            topic: "topic".to_string(),
            partition_key: None,
            payload: Value::Null,
            trace_id: None,
        }
    }

    fn relay(repo: MockOutboxRepository, publisher: MockMessagePublisher) -> OutboxRelay {
        OutboxRelay::new(Arc::new(repo), Arc::new(publisher))
    }

    #[tokio::test]
    async fn publishes_and_marks_each_pending_message() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let mut repo = MockOutboxRepository::new();
        repo.expect_unpublished()
            .times(1)
            .returning(move |_| Ok(vec![message(id1), message(id2)]));
        repo.expect_mark_published()
            .with(eq(id1))
            .times(1)
            .returning(|_| Ok(()));
        repo.expect_mark_published()
            .with(eq(id2))
            .times(1)
            .returning(|_| Ok(()));

        let mut publisher = MockMessagePublisher::new();
        publisher.expect_publish().times(2).returning(|_| Ok(()));

        relay(repo, publisher).publish_pending().await.unwrap();
    }

    #[tokio::test]
    async fn does_not_mark_when_publish_fails() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let mut repo = MockOutboxRepository::new();
        repo.expect_unpublished()
            .times(1)
            .returning(move |_| Ok(vec![message(id1), message(id2)]));
        // The failed row must NOT be marked, so it is retried next poll.
        repo.expect_mark_published().times(0);

        let mut publisher = MockMessagePublisher::new();
        // Only the first message is attempted; the relay stops on the error.
        publisher
            .expect_publish()
            .times(1)
            .returning(|_| Err(OutboxError::Kafka(KafkaError::Canceled)));

        let result = relay(repo, publisher).publish_pending().await;

        assert!(matches!(result, Err(OutboxError::Kafka(_))));
    }

    #[tokio::test]
    async fn does_nothing_when_outbox_is_empty() {
        let mut repo = MockOutboxRepository::new();
        repo.expect_unpublished().times(1).returning(|_| Ok(vec![]));
        repo.expect_mark_published().times(0);

        let mut publisher = MockMessagePublisher::new();
        publisher.expect_publish().times(0);

        relay(repo, publisher).publish_pending().await.unwrap();
    }
}
