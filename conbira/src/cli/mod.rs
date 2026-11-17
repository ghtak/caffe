use std::path::Path;

use conbira::config::Config;

use crate::option;

pub(crate) async fn run(options: option::CliOptions) -> espr::Result<()> {
    let option::CliOptions {
        common,
        generate_tls: tls_names,
        generate_ed25519: ed25519_requested,
    } = options;
    match (tls_names, ed25519_requested) {
        (Some(names), _) => {
            let config = Config::load(&common.config, common.env_file.as_deref())?;
            generate_tls(&config, &names)
        }
        (None, true) => {
            let config = Config::load(&common.config, common.env_file.as_deref())?;
            generate_ed25519(&config)
        }
        (None, false) => {
            for example in [
                "Examples:",
                "  conbira-cli --generate-tls localhost  Generate a TLS certificate and private key",
                "  conbira-cli --generate-ed25519        Generate an Ed25519 key pair for JWT signing",
            ] {
                println!("{example}");
            }
            Ok(())
        }
    }
}

fn generate_tls(config: &Config, names: &[String]) -> espr::Result<()> {
    let tls = config
        .https
        .as_ref()
        .and_then(|https| https.tls.as_ref())
        .ok_or_else(|| {
            espr::Error::InvalidParameter("HTTPS TLS configuration is required".into())
        })?;
    create_parent_directories(&[&tls.cert_path, &tls.key_path])?;
    espr::tls::generate(tls, names)?;
    Ok(())
}

fn generate_ed25519(config: &Config) -> espr::Result<()> {
    let keys = &config
        .jwt
        .as_ref()
        .ok_or_else(|| {
            espr::Error::InvalidParameter("JWT Ed25519 configuration is required".into())
        })?
        .ed25519;
    create_parent_directories(&[&keys.key_path, &keys.public_key_path])?;
    espr::jwt::ed25519::generate(keys)?;
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
