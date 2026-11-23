use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

/// Generates a self-signed certificate and private key for testing.
/// Overwrites existing files. Do not use in production.
pub fn generate(config: &Config, subject_alt_names: &[String]) -> crate::Result<()> {
    if config.cert_path == config.key_path {
        return Err(crate::Error::InvalidParameter(
            "certificate and private key paths must differ".into(),
        ));
    }
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(subject_alt_names.to_vec())
            .context("failed to generate TLS certificate")?;

    let mut key_options = OpenOptions::new();
    key_options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        key_options.mode(0o600);
    }
    let mut key = key_options.open(&config.key_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        key.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    key.write_all(signing_key.serialize_pem().as_bytes())?;
    std::fs::write(&config.cert_path, cert.pem())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    mod contract {
        use super::super::*;

        #[test]
        fn generates_and_overwrites_pem_files() {
            let dir = crate::test_support::TestDir::new("tls").unwrap();
            let config = Config {
                cert_path: dir.path().join("cert.pem"),
                key_path: dir.path().join("key.pem"),
            };
            std::fs::write(&config.cert_path, "old certificate").unwrap();
            std::fs::write(&config.key_path, "old key").unwrap();

            generate(&config, &["localhost".into(), "127.0.0.1".into()]).unwrap();
            // The original contents are replaced with certificate and key PEM.
            assert!(
                std::fs::read_to_string(&config.cert_path)
                    .unwrap()
                    .contains("BEGIN CERTIFICATE")
            );
            assert!(
                std::fs::read_to_string(&config.key_path)
                    .unwrap()
                    .contains("BEGIN PRIVATE KEY")
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                // The private key remains restricted to its owner.
                assert_eq!(
                    std::fs::metadata(&config.key_path)
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
            }
        }
    }
}
