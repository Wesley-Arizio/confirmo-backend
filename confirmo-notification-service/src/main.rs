use std::sync::Arc;

use clap::Parser;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::{BorrowedMessage, Headers};
use rdkafka::{ClientConfig, Message};

use confirmo_shared::auth_event::AuthEvent;
use tracing::Level;
use uuid::Uuid;

use crate::auth::AuthEventConsumer;
use crate::kafka::KafkaMessageProcessor;

mod auth;
mod kafka;

pub const TRACE_ID_HEADER: &str = "x-trace-id";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Comma-separated list of Kafka brokers (host:port)
    #[arg(long, env = "KAFKA_BROKERS")]
    kafka_brokers: String,

    /// Comma-separated list of Kafka topics to consume
    #[arg(long, env = "KAFKA_CONSUMER_TOPICS")]
    kafka_consumer_topics: String,

    /// Consumer group id this consumer belongs to
    #[arg(long, env = "KAFKA_CONSUMER_GROUP_ID")]
    kafka_consumer_group_id: String,

    /// SMTP username (usually the email address)
    #[arg(long, env = "SMTP_USERNAME")]
    smtp_username: String,

    /// SMTP password or app password
    #[arg(long, env = "SMTP_PASSWORD")]
    smtp_password: String,

    /// "From" address used in outgoing emails
    #[arg(long, env = "SMTP_FROM")]
    smtp_from: String,
}

fn get_trace_id(message: &BorrowedMessage<'_>) -> String {
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

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().expect("Failed to load .env file");
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .pretty()
        .init();

    let consumer: Arc<StreamConsumer> = Arc::new(
        ClientConfig::new()
            .set("group.id", args.kafka_consumer_group_id)
            .set("bootstrap.servers", args.kafka_brokers)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "false")
            .create()
            .expect("Could not create kafka consumer"),
    );

    consumer
        .subscribe(&[&args.kafka_consumer_topics])
        .expect("Could not subscribe to topics");

    let auth_event_consumer = Arc::new(
        AuthEventConsumer::new(args.smtp_username, args.smtp_from, args.smtp_password)
            .expect("Failed to initialize SMTP client"),
    );

    let processor = KafkaMessageProcessor::new(consumer.clone());

    loop {
        match consumer.recv().await {
            Ok(m) => {
                let bytes = match m.payload() {
                    Some(b) => b,
                    None => continue,
                };

                let event: AuthEvent = match serde_json::from_slice(bytes) {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::error!("Invalid Message: {:?}", e);
                        continue;
                    }
                };

                match event {
                    AuthEvent::EmailVerification(payload) => {
                        let trace_id = get_trace_id(&m);
                        processor
                            .process(&m, &trace_id, "email_verification", || async {
                                auth_event_consumer
                                    .send_email_verification(&payload.email, &payload.token)
                                    .await
                            })
                            .await;
                    }
                }
            }
            Err(e) => {
                tracing::error!("Kafka error: {:?}", e);
            }
        }
    }
}
