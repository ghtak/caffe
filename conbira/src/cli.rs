// Espresso con birra
mod conbira;
mod option;

use clap::Parser;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = option::Options::parse();
    match dotenvy::from_path(options.env_file.unwrap_or_else(|| ".env".into())) {
        Ok(_) => Ok(()),
        Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }?;

    let config: conbira::Config = espr::config::load(&options.config, None)?;

    if let Some(names) = &options.generate_tls {
        let tls = config
            .https
            .as_ref()
            .and_then(|https| https.tls.as_ref())
            .ok_or_else(|| {
                espr::Error::InvalidParameter("HTTPS TLS configuration is required".into())
            })?;
        create_parent_directories(&[&tls.cert_path, &tls.key_path])?;
        espr::tls::generate(tls, names)?;
        return Ok(());
    }

    if options.generate_ed25519 {
        let keys = &config
            .jwt
            .as_ref()
            .ok_or_else(|| {
                espr::Error::InvalidParameter("JWT Ed25519 configuration is required".into())
            })?
            .ed25519;
        create_parent_directories(&[&keys.key_path, &keys.public_key_path])?;
        espr::jwt::ed25519::generate(keys)?;
        return Ok(());
    }

    let guard = espr::tracing::initialize(&config.trace)?;

    let _container = conbira::Container::new(&config).await?;

    guard.shutdown().await?;
    Ok(())
}

fn create_parent_directories(paths: &[&Path]) -> std::io::Result<()> {
    for path in paths {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}
