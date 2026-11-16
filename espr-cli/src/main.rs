mod option;

use clap::Parser;

#[derive(serde::Deserialize, Debug)]
struct Config {
    trace: espr::tracing::Config,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = option::Options::parse();
    let env_file = options.env_file.unwrap_or_else(|| ".env".into());
    match dotenvy::from_path(env_file) {
        Ok(_) => {}
        Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let config: Config = espr::config::load(options.config, None)?;

    let _guard = espr::tracing::initialize(&config.trace)?;

    tracing::info!("hello");

    Ok(())
}
