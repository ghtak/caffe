use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;
use opentelemetry::trace::TraceContextExt as _;
use serde::Deserialize;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

use crate::tls;
use crate::validate::Validate;

pub fn make_request_span<B>(request: &axum::http::Request<B>) -> tracing::Span {
    let parent = crate::tracing::extract_trace_context(request.headers());
    let span = tracing::info_span!(parent: None, "http.request"
        , method = %request.method()
        , uri = %request.uri()
        , otel.kind = "server"
        , trace_id = tracing::field::Empty
        , span_id = tracing::field::Empty);
    if let Err(error) = span.set_parent(parent) {
        tracing::warn!(%error, "failed to set request span parent");
    }
    let context = span.context();
    let otel_span = context.span();
    let span_context = otel_span.span_context();
    if span_context.is_valid() {
        span.record("trace_id", tracing::field::display(span_context.trace_id()));
        span.record("span_id", tracing::field::display(span_context.span_id()));
    }
    span
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub tls: Option<tls::Config>,
    pub static_files: Option<StaticFiles>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StaticFiles {
    pub url_prefix: String,
    pub directory: PathBuf,
}

impl StaticFiles {
    pub fn as_router<S>(&self) -> axum::Router<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        axum::Router::new().nest_service(
            &self.url_prefix,
            tower_http::services::ServeDir::new(&self.directory),
        )
    }
}

impl Validate for StaticFiles {
    type Error = crate::Error;

    fn validate(&self) -> crate::Result<()> {
        if !self.url_prefix.starts_with('/') {
            return Err(crate::Error::Validation(
                "static files URL prefix must start with '/'".into(),
            ));
        }

        if self.url_prefix.contains(['?', '#']) {
            return Err(crate::Error::Validation(
                "static files URL prefix must not contain a query or fragment".into(),
            ));
        }

        if self.directory.as_os_str().is_empty() {
            return Err(crate::Error::Validation(
                "static files directory must not be empty".into(),
            ));
        }

        Ok(())
    }
}

pub struct Server {
    config: Config,
    tls_config: Option<RustlsConfig>,
}

impl Server {
    pub async fn new(config: &Config) -> crate::Result<Self> {
        let tls_config = match &config.tls {
            Some(tls) => Some(
                RustlsConfig::from_pem_file(&tls.cert_path, &tls.key_path)
                    .await
                    .context("failed to load TLS certificate and key")?,
            ),
            None => None,
        };

        Ok(Self {
            config: config.clone(),
            tls_config,
        })
    }

    pub async fn run(&self, app: axum::Router) -> crate::Result<()> {
        let listener = tokio::net::TcpListener::bind(&self.config.bind_addr)
            .await
            .with_context(|| format!("failed to bind listener to {}", self.config.bind_addr))?;
        let address = listener
            .local_addr()
            .context("failed to read bound listener address")?;
        let listener = listener.into_std().context("failed to convert listener")?;
        let handle = axum_server::Handle::new();
        let mut server: std::pin::Pin<
            Box<dyn std::future::Future<Output = std::io::Result<()>> + Send>,
        > = match &self.tls_config {
            Some(tls_config) => Box::pin(
                axum_server::from_tcp_rustls(listener, tls_config.clone())
                    .context("failed to configure server")?
                    .handle(handle.clone())
                    .serve(app.into_make_service()),
            ),
            None => Box::pin(
                axum_server::from_tcp(listener)
                    .context("failed to configure server")?
                    .handle(handle.clone())
                    .serve(app.into_make_service()),
            ),
        };

        tracing::info!(%address, "server listening");
        tokio::select! {
            result = &mut server => {
                result.context("server terminated unexpectedly")?;
            }
            () = shutdown_signal(address.clone()) => {
                handle.graceful_shutdown(Some(Duration::from_secs(30)));
                server.await.context("server shutdown failed")?;
            }
        }
        tracing::info!(%address,"server stopped");
        Ok(())
    }

    pub async fn reload_tls(&self) -> crate::Result<()> {
        let Some(tls_config) = &self.tls_config else {
            return Err(crate::Error::InvalidState("TLS is not enabled".into()));
        };

        let Some(cfg) = &self.config.tls else {
            unreachable!("TLS config must exist if TLS is enabled");
        };

        tls_config
            .reload_from_pem_file(&cfg.cert_path, &cfg.key_path)
            .await
            .context("failed to reload TLS certificate and key")?;

        tracing::info!("TLS certificate and key reloaded");
        Ok(())
    }
}

async fn shutdown_signal(address: SocketAddr) {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!(%error, "failed to listen for Ctrl-C");
        }
    };
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let terminate = async {
            match signal(SignalKind::terminate()) {
                Ok(mut stream) => {
                    stream.recv().await;
                }
                Err(error) => tracing::warn!(%error, "failed to listen for SIGTERM"),
            }
        };

        tokio::select! {
            () = ctrl_c => {},
            () = terminate => {},
        }
    }

    #[cfg(not(unix))]
    ctrl_c.await;

    tracing::info!(%address, "shutdown requested; graceful shutdown started");
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use axum::routing::get;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tower::ServiceExt as _;
    use tower_http::trace::TraceLayer;
    use tracing_subscriber::layer::SubscriberExt as _;

    #[test]
    fn request_span_records_trace_and_span_ids() {
        struct Capture(Arc<Mutex<Vec<(String, String)>>>);
        impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
            fn on_record(
                &self,
                _id: &tracing::span::Id,
                values: &tracing::span::Record<'_>,
                _ctx: tracing_subscriber::layer::Context<'_, S>,
            ) {
                struct Visitor<'a>(&'a Mutex<Vec<(String, String)>>);
                impl tracing::field::Visit for Visitor<'_> {
                    fn record_debug(
                        &mut self,
                        field: &tracing::field::Field,
                        value: &dyn std::fmt::Debug,
                    ) {
                        self.0
                            .lock()
                            .unwrap()
                            .push((field.name().to_owned(), format!("{value:?}")));
                    }
                }
                values.record(&mut Visitor(&self.0));
            }
        }
        let captured = Arc::new(Mutex::new(Vec::new()));
        let provider = SdkTracerProvider::builder().build();
        let subscriber = tracing_subscriber::registry()
            .with(Capture(captured.clone()))
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
        tracing::subscriber::with_default(subscriber, || {
            let request = Request::builder()
                .header(
                    "traceparent",
                    "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                )
                .body(Body::empty())
                .unwrap();
            let span = super::make_request_span(&request);
            let _guard = span.enter();
            let ids = crate::tracing::current_trace_ids().unwrap();
            assert_eq!(ids.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
            assert_eq!(
                *captured.lock().unwrap(),
                [
                    ("trace_id".into(), ids.trace_id),
                    ("span_id".into(), ids.span_id)
                ]
            );
        });
    }

    async fn trace_id(router: Router, request: Request<Body>) -> String {
        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        String::from_utf8(body.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn request_trace_id_is_propagated_or_generated() {
        let provider = SdkTracerProvider::builder().build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
        let _subscriber = tracing::subscriber::set_default(subscriber);

        let router = Router::new()
            .route(
                "/",
                get(|| async { crate::tracing::current_trace_ids().unwrap().trace_id }),
            )
            .layer(TraceLayer::new_for_http().make_span_with(super::make_request_span));

        let request = Request::builder()
            .uri("/")
            .header(
                "traceparent",
                "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            )
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            trace_id(router.clone(), request).await,
            "4bf92f3577b34da6a3ce929d0e0e4736"
        );

        let request = Request::builder().uri("/").body(Body::empty()).unwrap();
        let generated = trace_id(router, request).await;
        println!("generated trace_id: {generated}");
        assert_eq!(generated.len(), 32);
        assert!(generated.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(generated, "00000000000000000000000000000000");
        assert_ne!(generated, "4bf92f3577b34da6a3ce929d0e0e4736");
    }
}
