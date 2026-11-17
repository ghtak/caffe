mod option;

use std::path::Path;

use clap::Parser;
use espr::validate::Validate;
use tower_http::trace::TraceLayer;

#[derive(serde::Deserialize, Debug)]
struct Config {
    trace: espr::tracing::Config,
    http: espr::http::Config,
    https: espr::http::Config,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = option::Options::parse();
    load_dotenv(options.env_file.unwrap_or_else(|| ".env".into()))?;

    let config: Config = espr::config::load(&options.config, None)?;

    if let Some(names) = &options.generate_tls {
        let tls = config.https.tls.as_ref().ok_or_else(|| {
            espr::Error::InvalidParameter("HTTPS TLS configuration is required".into())
        })?;
        generate_tls(tls, names)?;
        return Ok(());
    }

    let guard = espr::tracing::initialize(&config.trace)?;

    let result = async {
        config.https.tls.as_ref().ok_or_else(|| {
            espr::Error::InvalidParameter("HTTPS TLS configuration is required".into())
        })?;

        let router =
            axum::Router::new().route("/", axum::routing::get(|| async { "Hello, World!" }));
        let ((http, http_router), (https, https_router)) = tokio::try_join!(
            prepare_server(config.http, router.clone()),
            prepare_server(config.https, router),
        )?;
        tokio::try_join!(http.run(http_router), https.run(https_router))?;
        Ok::<(), espr::Error>(())
    }
    .await;
    result.and(guard.shutdown().await)?;

    Ok(())
}

async fn prepare_server(
    config: espr::http::Config,
    router: axum::Router,
) -> espr::Result<(espr::http::Server, axum::Router)> {
    let router = match &config.static_files {
        Some(static_files) => {
            static_files.validate()?;
            router.merge(static_files.as_router())
        }
        None => router,
    };
    let router =
        router.layer(TraceLayer::new_for_http().make_span_with(espr::http::make_request_span));
    Ok((espr::http::Server::new(&config).await?, router))
}

fn load_dotenv<P: AsRef<Path>>(path: P) -> espr::Result<()> {
    match dotenvy::from_path(path) {
        Ok(_) => Ok(()),
        Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(espr::Error::Internal(error.into())),
    }
}

fn generate_tls(config: &espr::tls::Config, names: &[String]) -> espr::Result<()> {
    for path in [&config.cert_path, &config.key_path] {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
    }
    espr::tls::generate(config, names)?;
    Ok(())
}
