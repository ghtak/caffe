// Espresso con birra
use clap::Parser;
use conbira::config::Config;
use conbira::{app, option};

#[tokio::main]
async fn main() -> espr::Result<()> {
    let options = option::Options::parse();
    let config = Config::load(&options.config, options.env_file.as_deref())?;
    let guard = espr::tracing::initialize(&config.trace)?;
    let _container = app::Container::new(&config).await?;

    let _shutdown = guard.shutdown().await?;
    Ok(())
}
