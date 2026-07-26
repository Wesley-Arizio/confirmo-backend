pub mod auth_event;
pub mod constants;

#[cfg(feature = "grpc")]
pub mod auth {
    tonic::include_proto!("auth");
}

#[cfg(feature = "kafka")]
pub mod kafka;
