use std::sync::Arc;

use serde_json::Value;
use sqlx::{PgConnection, PgPool, types::Uuid};

use crate::error::OutboxError;

pub struct NewOutboxMessage {
    pub topic: String,
    pub partition_key: Option<String>,
    pub payload: Value,
    pub trace_id: Option<String>,
}

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct OutboxMessageDAO {
    pub id: Uuid,
    pub topic: String,
    pub partition_key: Option<String>,
    pub payload: Value,
    pub trace_id: Option<String>,
}

#[mockall::automock]
#[async_trait::async_trait]
pub trait OutboxRepository: Send + Sync + 'static {
    async fn enqueue(
        &self,
        conn: &mut PgConnection,
        message: NewOutboxMessage,
    ) -> Result<Uuid, OutboxError>;

    /// Oldest-first batch of rows not published yet.
    async fn unpublished(&self, limit: i64) -> Result<Vec<OutboxMessageDAO>, OutboxError>;

    /// Mark a row as published once Kafka has acknowledged it.
    async fn mark_published(&self, id: Uuid) -> Result<(), OutboxError>;
}

pub struct OutboxPostgresRepository {
    pub pg_pool: Arc<PgPool>,
}

impl OutboxPostgresRepository {
    pub fn new(pg_pool: Arc<PgPool>) -> Self {
        Self { pg_pool }
    }
}

#[async_trait::async_trait]
impl OutboxRepository for OutboxPostgresRepository {
    async fn enqueue(
        &self,
        conn: &mut PgConnection,
        message: NewOutboxMessage,
    ) -> Result<Uuid, OutboxError> {
        let id: Uuid = sqlx::query_scalar(r#"INSERT INTO outbox (topic, partition_key, payload, trace_id) VALUES ($1, $2, $3, $4) RETURNING id"#)
            .bind(message.topic)
            .bind(message.partition_key)
            .bind(message.payload)
            .bind(message.trace_id)
            .fetch_one(&mut *conn)
            .await?;

        Ok(id)
    }

    async fn unpublished(&self, limit: i64) -> Result<Vec<OutboxMessageDAO>, OutboxError> {
        let messages: Vec<OutboxMessageDAO> = sqlx::query_as(r#"SELECT id, topic, partition_key, payload, trace_id FROM outbox WHERE published_at IS NULL ORDER BY created_at LIMIT $1"#)
            .bind(limit)
            .fetch_all(&*self.pg_pool)
            .await?;

        Ok(messages)
    }

    async fn mark_published(&self, id: Uuid) -> Result<(), OutboxError> {
        sqlx::query(r#"UPDATE outbox SET published_at = now() WHERE id = $1"#)
            .bind(id)
            .execute(&*self.pg_pool)
            .await?;

        Ok(())
    }
}
