use ::argon2::{Algorithm, Argon2, Params, Version};
use password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier as _, SaltString};
use rand_core::OsRng;
use serde::Deserialize;

/// Parameters for Argon2id password hashing.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Memory cost in KiB.
    pub memory_kib: u32,
    /// Number of iterations.
    pub iterations: u32,
    /// Number of parallel lanes.
    pub parallelism: u32,
    /// Output length in bytes.
    pub output_len: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            memory_kib: 19 * 1024,
            iterations: 2,
            parallelism: 1,
            output_len: 32,
        }
    }
}

pub struct Hasher {
    inner: Argon2<'static>,
}

impl Hasher {
    pub fn new(config: &Config) -> crate::Result<Self> {
        let params = Params::new(
            config.memory_kib,
            config.iterations,
            config.parallelism,
            Some(config.output_len),
        )
        .map_err(|error| crate::Error::InvalidParameter(error.to_string().into()))?;
        Ok(Self {
            inner: Argon2::new(Algorithm::Argon2id, Version::V0x13, params),
        })
    }
}

impl super::PasswordHasher for Hasher {
    fn hash(&self, password: &str) -> crate::Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        self.inner
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|error| {
                crate::Error::Internal(anyhow::anyhow!("password hashing failed: {error}"))
            })
    }

    fn verify(&self, password: &str, hash: &str) -> crate::Result<bool> {
        let parsed = PasswordHash::new(hash).map_err(|error| {
            crate::Error::InvalidParameter(format!("invalid password hash: {error}").into())
        })?;
        match self.inner.verify_password(password.as_bytes(), &parsed) {
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
    mod contract {
        use super::super::*;
        use crate::hash::PasswordHasher;

        #[test]
        fn hashes_and_verifies_using_stored_parameters() {
            let hasher = Hasher::new(&Config {
                memory_kib: 32,
                iterations: 2,
                parallelism: 1,
                output_len: 32,
            })
            .unwrap();
            let hash = hasher.hash("password").unwrap();
            // Repeated hashes use different salts.
            assert_ne!(hash, hasher.hash("password").unwrap());
            let verifier = Hasher::new(&Config::default()).unwrap();
            // The stored parameters verify the right password and reject
            // incorrect inputs.
            assert!(verifier.verify("password", &hash).unwrap());
            assert!(!verifier.verify("wrong", &hash).unwrap());
            assert!(verifier.verify("password", "invalid").is_err());
        }
    }

    mod edge {
        use super::super::*;

        #[test]
        fn rejects_invalid_parameters() {
            // Zero parameters are rejected at construction.
            assert!(
                Hasher::new(&Config {
                    output_len: 0,
                    ..Config::default()
                })
                .is_err()
            );
        }
    }
}
