use ::pbkdf2::{Algorithm, Params, Pbkdf2};
use password_hash::{Output, PasswordHash, PasswordHasher as _, PasswordVerifier as _, SaltString};
use rand_core::OsRng;
use serde::Deserialize;

/// Parameters for PBKDF2-HMAC-SHA-256 password hashing.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Number of PBKDF2 iterations.
    pub iterations: u32,
    /// Output length in bytes.
    pub output_len: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            iterations: 600_000,
            output_len: 32,
        }
    }
}

pub struct Hasher {
    params: Params,
}

impl Hasher {
    pub fn new(config: &Config) -> crate::Result<Self> {
        if config.iterations == 0
            || !(Output::MIN_LENGTH..=Output::MAX_LENGTH).contains(&config.output_len)
        {
            return Err(crate::Error::InvalidParameter(
                "invalid PBKDF2 parameters".into(),
            ));
        }
        Ok(Self {
            params: Params {
                rounds: config.iterations,
                output_length: config.output_len,
            },
        })
    }
}

impl super::PasswordHasher for Hasher {
    fn hash(&self, password: &str) -> crate::Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        Pbkdf2
            .hash_password_customized(
                password.as_bytes(),
                Some(Algorithm::Pbkdf2Sha256.ident()),
                None,
                self.params,
                &salt,
            )
            .map(|hash| hash.to_string())
            .map_err(|error| {
                crate::Error::Internal(anyhow::anyhow!("password hashing failed: {error}"))
            })
    }

    fn verify(&self, password: &str, hash: &str) -> crate::Result<bool> {
        let parsed = PasswordHash::new(hash).map_err(|error| {
            crate::Error::InvalidParameter(format!("invalid password hash: {error}").into())
        })?;
        if parsed.algorithm != Algorithm::Pbkdf2Sha256.ident() {
            return Err(crate::Error::InvalidParameter(
                "unsupported password hash algorithm".into(),
            ));
        }
        let params = Params::try_from(&parsed).map_err(|error| {
            crate::Error::InvalidParameter(format!("invalid password hash: {error}").into())
        })?;
        if params.rounds == 0 {
            return Err(crate::Error::InvalidParameter(
                "invalid PBKDF2 iterations".into(),
            ));
        }
        match Pbkdf2.verify_password(password.as_bytes(), &parsed) {
            Ok(()) => Ok(true),
            Err(password_hash::Error::Password) => Ok(false),
            Err(error) => Err(crate::Error::InvalidParameter(
                format!("invalid password hash: {error}").into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::PasswordHasher;

    #[test]
    fn hashes_and_verifies_using_stored_parameters() {
        let hasher = Hasher::new(&Config {
            iterations: 10,
            output_len: 24,
        })
        .unwrap();
        let hash = hasher.hash("password").unwrap();
        assert_ne!(hash, hasher.hash("password").unwrap());
        let verifier = Hasher::new(&Config::default()).unwrap();
        assert!(verifier.verify("password", &hash).unwrap());
        assert!(!verifier.verify("wrong", &hash).unwrap());
        assert!(verifier.verify("password", "invalid").is_err());
        assert!(
            verifier
                .verify("password", &hash.replace("i=10", "i=0"))
                .is_err()
        );
        assert!(
            verifier
                .verify("password", &hash.replace("pbkdf2-sha256", "pbkdf2-sha512"))
                .is_err()
        );
    }

    #[test]
    fn rejects_invalid_parameters() {
        assert!(
            Hasher::new(&Config {
                iterations: 0,
                ..Config::default()
            })
            .is_err()
        );
        assert!(
            Hasher::new(&Config {
                output_len: 0,
                ..Config::default()
            })
            .is_err()
        );
    }
}
