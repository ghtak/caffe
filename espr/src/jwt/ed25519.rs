use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use anyhow::Context;
use serde::Deserialize;

/// PEM-encoded PKCS #8 private key and SubjectPublicKeyInfo public key paths.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// PKCS #8 private key path.
    pub key_path: PathBuf,
    /// SubjectPublicKeyInfo public key path.
    pub public_key_path: PathBuf,
}

/// Generate an Ed25519 key pair, overwriting existing files.
pub fn generate(config: &Config) -> crate::Result<()> {
    let private_key_path = &config.key_path;
    if private_key_path == &config.public_key_path {
        return Err(crate::Error::InvalidParameter(
            "private and public key paths must differ".into(),
        ));
    }
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)
        .context("failed to generate Ed25519 key pair")?;
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut private_key = options.open(private_key_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        private_key.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    private_key.write_all(key.serialize_pem().as_bytes())?;
    std::fs::write(&config.public_key_path, key.public_key_pem())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_private_key_path() {
        let config: Config = crate::config::load_str(
            "key_path = 'private.pem'\npublic_key_path = 'public.pem'",
            config::FileFormat::Toml,
            None,
        )
        .unwrap();

        assert_eq!(config.key_path, std::path::Path::new("private.pem"));
    }

    #[test]
    fn generates_matching_ed25519_pem_keys() {
        let dir = std::env::temp_dir().join(format!(
            "espr-jwt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let private_key_path = dir.join("private.pem");
        let config = Config {
            key_path: private_key_path.clone(),
            public_key_path: dir.join("public.pem"),
        };
        std::fs::write(&private_key_path, "old private key").unwrap();
        std::fs::write(&config.public_key_path, "old public key").unwrap();

        generate(&config).unwrap();

        let private_pem = std::fs::read_to_string(&private_key_path).unwrap();
        let public_pem = std::fs::read_to_string(&config.public_key_path).unwrap();
        let key = rcgen::KeyPair::from_pem(&private_pem).unwrap();
        assert_eq!(key.algorithm(), &rcgen::PKCS_ED25519);
        assert_eq!(key.public_key_pem(), public_pem);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::metadata(&private_key_path).unwrap().permissions();
            assert_eq!(permissions.mode() & 0o777, 0o600);
        }

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_identical_key_paths() {
        let config = Config {
            key_path: "key.pem".into(),
            public_key_path: "key.pem".into(),
        };

        assert!(matches!(
            generate(&config),
            Err(crate::Error::InvalidParameter(_))
        ));
    }
}
