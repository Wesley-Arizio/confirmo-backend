use std::convert::Infallible;
use tonic::body::Body;
use tower::{Layer, Service};
use tracing::Instrument;
use uuid::Uuid;

pub const TRACE_ID_HEADER: &str = "x-trace-id";

#[derive(Clone)]
pub struct GrpcTracingService<S> {
    inner: S,
}

#[derive(Clone)]
pub struct TracingLayer {}

impl<S> Layer<S> for TracingLayer {
    type Service = GrpcTracingService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GrpcTracingService { inner }
    }
}

impl<S> Service<http::Request<Body>> for GrpcTracingService<S>
where
    S: Service<http::Request<Body>, Error = Infallible> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;

    type Error = Infallible;

    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: http::Request<Body>) -> Self::Future {
        let trace_id = request
            .headers()
            .get(TRACE_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let span = tracing::info_span!(
            "auth_grpc_request",
            trace_id = %trace_id
        );

        let fut = self.inner.call(request);

        Box::pin(async move { fut.instrument(span).await })
    }
}
