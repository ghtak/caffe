use std::path::PathBuf;

use anyhow::Context;
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use opentelemetry::propagation::{Extractor, Injector, TextMapPropagator as _};
use opentelemetry::trace::{TraceContextExt as _, TracerProvider as _};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use serde::Deserialize;
use tracing_appender::non_blocking::{NonBlockingBuilder, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, Layer as _, Registry, fmt};

pub mod exporter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceIds {
    pub trace_id: String,
    pub span_id: String,
}

pub fn current_trace_ids() -> Option<TraceIds> {
    let context = tracing::Span::current().context();
    let span = context.span();
    let span_context = span.span_context();
    span_context.is_valid().then(|| TraceIds {
        trace_id: span_context.trace_id().to_string(),
        span_id: span_context.span_id().to_string(),
    })
}

pub fn inject_trace_context(headers: &mut HeaderMap) {
    headers.remove("traceparent");
    headers.remove("tracestate");
    let context = tracing::Span::current().context();
    TraceContextPropagator::new().inject_context(&context, &mut HeaderInjector(headers));
}

pub(crate) fn extract_trace_context(headers: &HeaderMap) -> opentelemetry::Context {
    TraceContextPropagator::new()
        .extract_with_context(&opentelemetry::Context::new(), &HeaderExtractor(headers))
}

struct HeaderExtractor<'a>(&'a HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key)?.to_str().ok()
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(HeaderName::as_str).collect()
    }
}

struct HeaderInjector<'a>(&'a mut HeaderMap);

impl Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        if key == "tracestate" && value.is_empty() {
            return;
        }
        if let (Ok(name), Ok(value)) = (HeaderName::try_from(key), HeaderValue::try_from(value)) {
            self.0.insert(name, value);
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    pub console: Option<Console>,
    pub file: Option<File>,
    pub exporter: Option<exporter::Config>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Console {
    pub filter: Option<String>,
    #[serde(default = "default_buffered_lines_limit")]
    pub buffered_lines_limit: usize,
    #[serde(default = "default_lossy")]
    pub lossy: bool,
    #[serde(default)]
    pub ansi: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct File {
    pub filter: Option<String>,
    pub directory: PathBuf,
    pub filename_prefix: String,
    #[serde(default = "default_buffered_lines_limit")]
    pub buffered_lines_limit: usize,
    #[serde(default = "default_lossy")]
    pub lossy: bool,
}

fn default_buffered_lines_limit() -> usize {
    tracing_appender::non_blocking::DEFAULT_BUFFERED_LINES_LIMIT
}

fn default_lossy() -> bool {
    true
}

#[must_use = "keep this guard alive until application shutdown"]
pub struct TracingGuard {
    _workers: Vec<WorkerGuard>,
    provider: Option<SdkTracerProvider>,
}

impl Drop for TracingGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            let _ = provider.shutdown();
        }
    }
}

impl TracingGuard {
    /// Flushes pending spans without blocking the async runtime thread.
    pub async fn shutdown(mut self) -> crate::Result<()> {
        let provider = self
            .provider
            .take()
            .expect("provider is present until shutdown");
        tokio::task::spawn_blocking(move || provider.shutdown())
            .await
            .map_err(|error| anyhow::anyhow!(error).context("trace shutdown task failed"))?
            .map_err(|error| anyhow::anyhow!(error).context("trace provider shutdown failed"))?;
        Ok(())
    }
}

pub fn initialize(config: &Config) -> crate::Result<TracingGuard> {
    let mut guards = Vec::new();
    let mut builder = SdkTracerProvider::builder();
    if let Some(exporter) = &config.exporter {
        builder = exporter::configure(builder, exporter)?;
    }
    let provider = builder.build();
    let tracer = provider.tracer("espr");

    let console = config.console.as_ref().map(|v| {
        let filter = resolve_filter(v.filter.as_deref());
        let (writer, guard) = NonBlockingBuilder::default()
            .buffered_lines_limit(v.buffered_lines_limit)
            .lossy(v.lossy)
            .finish(std::io::stderr());
        guards.push(guard);

        fmt::layer()
            .with_writer(writer)
            .compact()
            .with_ansi(v.ansi)
            .with_filter(filter)
    });

    let file = config
        .file
        .as_ref()
        .map(|v| -> anyhow::Result<_> {
            let filter = resolve_filter(v.filter.as_deref());

            let appender = RollingFileAppender::builder()
                .rotation(Rotation::DAILY)
                .filename_prefix(&v.filename_prefix)
                .filename_suffix("log") // 💡 추천: 날짜 뒤에 .log 확장자 붙이기
                .build(&v.directory)
                .with_context(|| {
                    format!(
                        "failed to initialize log file `{}` in `{}`",
                        v.filename_prefix,
                        v.directory.display()
                    )
                })?;

            let (writer, guard) = NonBlockingBuilder::default()
                .buffered_lines_limit(v.buffered_lines_limit)
                .lossy(v.lossy)
                .finish(appender);
            guards.push(guard);

            Ok(fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_filter(filter))
        })
        .transpose()?;

    Registry::default()
        .with(console)
        .with(file)
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .try_init()
        .context("failed to install the global tracing subscriber")?;

    Ok(TracingGuard {
        _workers: guards,
        provider: Some(provider),
    })
}

fn resolve_filter(filter: Option<&str>) -> EnvFilter {
    EnvFilter::try_from_default_env()
        .ok()
        .or_else(|| filter.and_then(|d| EnvFilter::try_new(d).ok()))
        .unwrap_or_else(|| EnvFilter::new("info"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exporter_requires_tokio_runtime() {
        let config = Config {
            exporter: Some(exporter::Config {
                endpoint: "http://localhost:4317".into(),
            }),
            ..Config::default()
        };
        let error = match initialize(&config) {
            Ok(_) => panic!("exporter must require a runtime"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("requires a Tokio runtime"));
    }

    #[test]
    fn propagates_request_context() {
        let provider = SdkTracerProvider::builder().build();
        let subscriber = Registry::default()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
        tracing::subscriber::with_default(subscriber, || {
            assert_eq!(current_trace_ids(), None);
            let request = axum::http::Request::builder()
                .header(
                    "traceparent",
                    "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                )
                .header("tracestate", "vendor=value")
                .body(())
                .unwrap();
            let span = crate::http::make_request_span(&request);
            let _entered = span.enter();
            let ids = current_trace_ids().unwrap();
            assert_eq!(ids.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
            assert_ne!(ids.span_id, "00f067aa0ba902b7");
            let mut headers = HeaderMap::new();
            inject_trace_context(&mut headers);
            assert_eq!(
                headers["traceparent"],
                format!("00-{}-{}-01", ids.trace_id, ids.span_id)
            );
            assert_eq!(headers["tracestate"], "vendor=value");
        });
    }

    #[test]
    fn invalid_parent_starts_new_trace() {
        let provider = SdkTracerProvider::builder().build();
        let subscriber = Registry::default()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
        tracing::subscriber::with_default(subscriber, || {
            let request = axum::http::Request::builder()
                .header("traceparent", "invalid")
                .header("tracestate", "vendor=value")
                .body(())
                .unwrap();
            let span = crate::http::make_request_span(&request);
            let _entered = span.enter();
            assert!(current_trace_ids().is_some());
            let mut headers = HeaderMap::new();
            inject_trace_context(&mut headers);
            assert!(headers.contains_key("traceparent"));
            assert!(!headers.contains_key("tracestate"));
        });
    }

    #[test]
    fn initializes_file_logging() {
        const LOG_DIR: &str = "ESPR_TRACING_TEST_DIR";

        if let Some(directory) = std::env::var_os(LOG_DIR) {
            let config = Config {
                console: None,
                exporter: None,
                file: Some(File {
                    filter: Some("error".into()),
                    directory: directory.into(),
                    filename_prefix: "test".into(),
                    buffered_lines_limit: default_buffered_lines_limit(),
                    lossy: false,
                }),
            };
            let guard = initialize(&config).unwrap();
            let span = tracing::info_span!("request");
            assert!(span.context().span().span_context().is_valid());
            tracing::debug!("visible debug event");
            tracing::trace!("hidden trace event");
            assert!(initialize(&config).is_err());
            drop(guard);
            return;
        }

        let directory = std::env::temp_dir().join(format!(
            "espr-tracing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tracing::tests::initializes_file_logging"])
            .env(LOG_DIR, &directory)
            .env("RUST_LOG", "debug")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let log_files: Vec<_> = std::fs::read_dir(&directory)
            .expect("read log directory")
            .map(|entry| entry.expect("read log directory entry").path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("test.") && name.ends_with(".log"))
            })
            .collect();
        assert_eq!(log_files.len(), 1, "expected one test log file");
        let contents = std::fs::read_to_string(&log_files[0]).expect("read test log file");
        std::fs::remove_dir_all(directory).unwrap();

        assert!(contents.contains("visible debug event"));
        assert!(!contents.contains("hidden trace event"));
    }
}
