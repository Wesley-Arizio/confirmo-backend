fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("CARGO_FEATURE_GRPC").is_some() {
        #[cfg(feature = "grpc")]
        tonic_prost_build::configure()
            .compile_protos(&["proto/auth/auth.proto"], &["proto/auth", "/usr/include"])?;
    }

    Ok(())
}
