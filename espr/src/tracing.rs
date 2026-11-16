use std::path::PathBuf;

use anyhow::Context;
use serde::Deserialize;
use tracing_appender::non_blocking::{NonBlockingBuilder, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, Layer as _, Registry, fmt};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    pub console: Option<Console>,
    pub file: Option<File>,
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

fn default_lossy() -> bool { true }

#[must_use = "keep this guard alive until application shutdown"]
pub struct TracingGuard {
    _workers: Vec<WorkerGuard>,
}

pub fn initialize(config: &Config) -> crate::Result<TracingGuard> {
    let mut guards = Vec::new();
    if config.console.is_none() && config.file.is_none() {
        return Ok(TracingGuard { _workers: guards });
    }

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
        .try_init()
        .context("failed to install the global tracing subscriber")?;

    Ok(TracingGuard { _workers: guards })
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
    fn initializes_file_logging() {
        const LOG_DIR: &str = "ESPR_TRACING_TEST_DIR";

        if let Some(directory) = std::env::var_os(LOG_DIR) {
            // An empty configuration must not occupy the global subscriber.
            drop(initialize(&Config::default()).unwrap());

            let config = Config {
                console: None,
                file: Some(File {
                    filter: Some("error".into()),
                    directory: directory.into(),
                    filename_prefix: "test".into(),
                    buffered_lines_limit: default_buffered_lines_limit(),
                    lossy: false,
                }),
            };
            let guard = initialize(&config).unwrap();
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
