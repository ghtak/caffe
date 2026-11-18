use std::sync::Arc;

// Espresso con birra
use clap::Parser;
use conbira::config::Config;
use conbira::error::ResultExt;
use conbira::{app, option};

#[tokio::main]
async fn main() -> conbira::Result<()> {
    let options = option::Options::parse();
    let config = Config::load(&options.config, options.env_file.as_deref())?;
    let guard = espr::tracing::initialize(&config.trace)
        .map_err_internal("Failed to initialize tracing", conbira::here!())?;

    let container = Arc::new(app::Container::new(&config).await?);

    let router = app::http::router(container.clone())?;

    container
        .http_server
        .run(router)
        .await
        .map_err_internal("Failed to run http server", conbira::here!())?;

    guard
        .shutdown()
        .await
        .map_err_internal("Failed to shut down tracing", conbira::here!())?;

    Ok(())
}
