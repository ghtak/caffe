use opentelemetry_otlp::WithExportConfig as _;
use opentelemetry_sdk::trace::TracerProviderBuilder;
use serde::Deserialize;

/// Configuration for exporting spans to an OTLP endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub endpoint: String,
}

/// Configures batched OTLP/gRPC export. Requires a running Tokio runtime.
pub(super) fn configure(
    builder: TracerProviderBuilder,
    config: &Config,
) -> crate::Result<TracerProviderBuilder> {
    tokio::runtime::Handle::try_current()
        .map_err(|_| crate::Error::IllegalState("OTLP exporter requires a Tokio runtime".into()))?;
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.endpoint)
        .build()
        .map_err(|error| anyhow::anyhow!(error).context("failed to configure OTLP exporter"))?;
    Ok(builder.with_batch_exporter(exporter))
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::trace::{Span as _, Tracer as _, TracerProvider as _};
    use opentelemetry_proto::tonic::collector::trace::v1::{
        ExportTraceServiceRequest, ExportTraceServiceResponse,
        trace_service_server::{TraceService, TraceServiceServer},
    };
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::TcpListenerStream;

    struct Collector(mpsc::UnboundedSender<ExportTraceServiceRequest>);

    #[tonic::async_trait]
    impl TraceService for Collector {
        async fn export(
            &self,
            request: tonic::Request<ExportTraceServiceRequest>,
        ) -> Result<tonic::Response<ExportTraceServiceResponse>, tonic::Status> {
            self.0.send(request.into_inner()).unwrap();
            Ok(tonic::Response::new(ExportTraceServiceResponse::default()))
        }
    }

    #[tokio::test]
    async fn exports_spans_on_shutdown() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (tx, mut rx) = mpsc::unbounded_channel();
        let server = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(TraceServiceServer::new(Collector(tx)))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        let provider = configure(SdkTracerProvider::builder(), &Config { endpoint })
            .unwrap()
            .build();
        let mut span = provider.tracer("test").start("export-test");
        span.end();
        tokio::task::spawn_blocking(move || provider.shutdown().unwrap())
            .await
            .unwrap();
        let request = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            request.resource_spans[0].scope_spans[0].spans[0].name,
            "export-test"
        );
        server.abort();
    }
}
