pub mod auth_event;

#[cfg(feature = "grpc")]
pub mod auth {
    tonic::include_proto!("auth");
}

pub mod constants;

#[cfg(feature = "kafka")]
pub mod kafka;
