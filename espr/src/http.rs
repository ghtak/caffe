use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;
use serde::Deserialize;

use crate::tls;
use crate::validate::Validate;

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
    pub fn as_router(&self) -> axum::Router {
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
    pub async fn new(config: Config) -> crate::Result<Self> {
        let tls_config = match &config.tls {
            Some(tls) => Some(
                RustlsConfig::from_pem_file(&tls.cert_path, &tls.key_path)
                    .await
                    .context("failed to load TLS certificate and key")?,
            ),
            None => None,
        };

        Ok(Self { config, tls_config })
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
            return Err(crate::Error::IllegalState("TLS is not enabled".into()));
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
