mod domain;

use crate::application::core_service::CoreService;
use crate::application::ports::{AwsS3Presigner, GrpcAuthApi, KafkaMessageProducer};
use crate::database::lawyer::LawyerPostgresRepository;
use crate::database::submission::SubmissionPostgresRepository;
use crate::graphql_api::schema::{AppState, GraphqlContext, Schema, create_schema};
use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::web::Data;
use actix_web::{
    App, HttpMessage, HttpRequest, HttpResponse, HttpServer, Responder, get, route, web,
};
use clap::Parser;
use confirmo_shared::auth::auth_service_client::AuthServiceClient;
use confirmo_shared::kafka::{KafkaMessageProcessor, get_trace_id};
use juniper::http::GraphQLRequest;
use juniper::http::graphiql::graphiql_source;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::{ClientConfig, Message};
use sqlx::postgres::PgPoolOptions;
use sqlx::types::Uuid;
use std::sync::Arc;
use tonic::transport::Endpoint;
use tracing::{Instrument, Level, info_span};

mod application;
mod constants;
mod database;
mod graphql_api;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    // Port in which the server is being exposed to
    #[arg(long, env = "SERVER_PORT")]
    port: u16,

    // Port in which the server is being exposed to
    #[arg(long, env = "AUTH_CONNNECTION_URL")]
    auth_connection_url: String,

    /// Database URL Connection
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    /// Consumer group id this consumer belongs to
    #[arg(long, env = "KAFKA_CONSUMER_GROUP_ID")]
    kafka_consumer_group_id: String,

    /// Comma-separated list of Kafka brokers (host:port)
    #[arg(long, env = "KAFKA_BROKERS")]
    kafka_brokers: String,

    /// Comma-separated list of Kafka topics to consume
    #[arg(long, env = "KAFKA_CONSUMER_TOPICS")]
    kafka_consumer_topics: String,

    /// The AWS Region.
    #[structopt(long, env = "AWS_REGION")]
    aws_region: String,

    /// The name of the bucket.
    #[structopt(long, env = "AWS_BUCKET")]
    aws_bucket: String,

    /// The name of the bucket.
    #[structopt(long, env = "KAFKA_TOPIC_CORE_PROFILE_REVIEW_REQUESTED")]
    kafka_profile_review_requested: String,
}

/// GraphiQL playground UI
#[get("/graphiql")]
async fn graphql_playground() -> impl Responder {
    web::Html::new(graphiql_source("/graphql", None))
}

pub async fn tracing_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    let trace_id = req
        .headers()
        .get(constants::TRACE_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let span = info_span!("core_graphql_request", trace_id = %trace_id);
    req.extensions_mut().insert(trace_id);
    next.call(req).instrument(span).await
}

/// GraphQL endpoint
#[route("/graphql", method = "GET", method = "POST")]
async fn graphql(
    state: web::Data<AppState>,
    st: web::Data<Schema>,
    data: web::Json<GraphQLRequest>,
    request: HttpRequest,
) -> impl Responder {
    let trace_id = request
        .extensions()
        .get::<String>()
        .cloned()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let context = web::Data::new(GraphqlContext {
        app_state: state.clone(),
        trace_id,
    });
    let user = data.execute(&st, &context).await;
    HttpResponse::Ok().json(user)
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum CoreEvent {
    EmailVerified(EmailVerifiedPayload),
    ProfileReviewRequested(ProfileReviewRequestedPayload),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmailVerifiedPayload {
    pub user_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileReviewRequestedPayload {
    user_id: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().expect("Failed to load .env file");
    let args = Args::parse();

    tracing_subscriber::fmt()
        .pretty()
        .with_max_level(Level::DEBUG)
        .init();

    let schema = Arc::new(create_schema());

    let auth_connection_url = Endpoint::from_shared(args.auth_connection_url)
        .expect("Could not parse auth connection url to a valid tonic endpoint");

    let auth_client = AuthServiceClient::connect(auth_connection_url)
        .await
        .expect("Could not connect to auth microservice via grpc");

    let pg_pool = Arc::new(
        PgPoolOptions::new()
            .connect(&args.database_url)
            .await
            .expect("Could not connect with database"),
    );

    let grpc_auth_api = Arc::new(GrpcAuthApi::new(auth_client.clone()));
    let lawyer_repository = Arc::new(LawyerPostgresRepository::new(pg_pool.clone()));
    let submission_repository = Arc::new(SubmissionPostgresRepository::new(pg_pool.clone()));
    let s3_presigner = Arc::new(AwsS3Presigner::new(args.aws_bucket, args.aws_region).await);
    let producer = Arc::new(KafkaMessageProducer::new(
        &args.kafka_brokers,
        args.kafka_profile_review_requested.clone(),
    ));
    let core_service: Arc<CoreService> = Arc::new(CoreService::new(
        lawyer_repository.clone(),
        submission_repository,
        s3_presigner,
        producer,
        grpc_auth_api,
    ));
    let kafka_consumer_group_id = args.kafka_consumer_group_id.clone();
    let kafka_brokers = args.kafka_brokers.clone();
    let kafka_consumer_topics = args.kafka_consumer_topics.clone();

    let core_service_kafka = core_service.clone();
    let kafka_profile_review_requested = args.kafka_profile_review_requested;
    let consumer_taks = tokio::task::spawn(async move {
        let core_service = core_service_kafka.clone();
        let consumer: Arc<StreamConsumer> = Arc::new(
            ClientConfig::new()
                .set("group.id", kafka_consumer_group_id)
                .set("bootstrap.servers", kafka_brokers)
                .set("enable.partition.eof", "false")
                .set("session.timeout.ms", "6000")
                .set("enable.auto.commit", "false")
                .create()
                .expect("Could not create kafka consumer"),
        );

        consumer
            .subscribe(&[&kafka_consumer_topics, &kafka_profile_review_requested])
            .expect("Could not subscribe to topics");

        let processor = KafkaMessageProcessor::new(consumer.clone());
        loop {
            match consumer.recv().await {
                Ok(m) => {
                    let bytes = match m.payload() {
                        Some(b) => b,
                        None => continue,
                    };

                    let event: CoreEvent = match serde_json::from_slice(bytes) {
                        Ok(e) => e,
                        Err(e) => {
                            tracing::error!("Invalid Message: {:?}", e);
                            continue;
                        }
                    };

                    match event {
                        CoreEvent::EmailVerified(payload) => {
                            let trace_id = get_trace_id(&m);
                            processor
                                .process(&m, &trace_id, "email_verified", || async {
                                    core_service.email_verified(&payload.user_id).await
                                })
                                .await;
                        }
                        CoreEvent::ProfileReviewRequested(payload) => {
                            let trace_id = get_trace_id(&m);
                            processor
                                .process(&m, &trace_id, "profile_review_requested", || async {
                                    core_service.profile_review_requested(payload.user_id).await
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
    });

    let app_state = web::Data::new(AppState::new(core_service));
    HttpServer::new(move || {
        App::new()
            .wrap(actix_web::middleware::from_fn(tracing_middleware))
            .app_data(app_state.clone())
            .app_data(Data::from(schema.clone()))
            .service(graphql_playground)
            .service(graphql)
    })
    .bind(("127.0.0.1", args.port))?
    .run()
    .await?;

    let _ = consumer_taks.await?;

    Ok(())
}
