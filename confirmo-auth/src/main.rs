use std::sync::Arc;

use clap::Parser;
use confirmo_shared::auth::auth_service_server::AuthServiceServer;
use rdkafka::{ClientConfig, producer::FutureProducer};
use sqlx::postgres::PgPoolOptions;
use tonic::transport::Server;
use tracing::Level;

use crate::{
    database::credentials::CredentialsPostgresRepo, interceptor::TracingLayer, server::AuthServer,
};

mod database;
mod error;
mod interceptor;
mod password;
mod server;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Port in which the server is being exposed to
    #[arg(long, env = "SERVER_PORT")]
    port: u16,

    /// Database URL Connection
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    /// Secret used to hash account verification code
    #[arg(long, env = "HASH_SECRET")]
    hash_secret: String,

    /// Host/port pairs used to establish the initial connection to the Kafka cluster.
    #[arg(long, env = "KAFKA_BROKERS")]
    kafka_brokers: String,

    /// Kafka producer for email verification
    #[arg(long, env = "KAFKA_TOPIC_AUTH_EMAIL_VERIFICATION")]
    kafka_auth_email_verification: String,

    /// Kafka producer for email verified
    #[arg(long, env = "KAFKA_TOPIC_AUTH_EMAIL_VERIFIED")]
    kafka_auth_email_verified: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().expect("Failed to load .env file");
    let args = Args::parse();

    tracing_subscriber::fmt()
        .pretty()
        .with_max_level(Level::DEBUG)
        .init();

    let addr = format!("127.0.0.1:{}", args.port)
        .parse()
        .expect("Could not parse server address");

    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", args.kafka_brokers)
        .set("queue.buffering.max.ms", "0") // Do not buffer
        .create()
        .expect("Producer creation failed");

    let pg_pool = PgPoolOptions::new()
        .connect(&args.database_url)
        .await
        .expect("Could not connect with database");

    let credentials_repo = CredentialsPostgresRepo::new(Arc::new(pg_pool));

    let auth_service = AuthServer::new(
        credentials_repo,
        args.hash_secret,
        producer,
        args.kafka_auth_email_verification,
        args.kafka_auth_email_verified,
    );
    tracing::info!("🚀 Starting gRPC server on {}", addr);
    Server::builder()
        .layer(TracingLayer {})
        .add_service(AuthServiceServer::new(auth_service))
        .serve(addr)
        .await?;

    Ok(())
}
