mod option;

use std::path::Path;

use clap::Parser;

#[derive(serde::Deserialize, Debug)]
struct Config {
    trace: espr::tracing::Config,
    tls: espr::tls::Config,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = option::Options::parse();
    load_dotenv(options.env_file.unwrap_or_else(|| ".env".into()))?;

    let config: Config = espr::config::load(&options.config, None)?;

    if let Some(names) = &options.generate_tls {
        generate_tls(&config.tls, names)?;
        return Ok(());
    }

    let _guard = espr::tracing::initialize(&config.trace)?;

    tracing::info!("hello");

    Ok(())
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
