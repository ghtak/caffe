// Espresso con birra
mod conbira;
mod option;

use clap::Parser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = option::Options::parse();
    match dotenvy::from_path(options.env_file.unwrap_or_else(|| ".env".into())) {
        Ok(_) => Ok(()),
        Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }?;
    let config: conbira::Config = espr::config::load(&options.config, None)?;
    let guard = espr::tracing::initialize(&config.trace)?;

    guard.shutdown().await?;
    Ok(())
}
