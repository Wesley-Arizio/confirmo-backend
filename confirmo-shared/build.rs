fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "grpc")]
    tonic_prost_build::configure().compile_protos(&["proto/auth/auth.proto"], &["proto/auth"])?;

    Ok(())
}
