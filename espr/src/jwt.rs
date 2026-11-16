pub mod ed25519;

use anyhow::Context;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Access token claims. `iat` and `exp` are Unix timestamps in seconds.
/// User claims are visible to anyone holding the token.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Claims {
    pub sub: String,
    pub iss: String,
    pub aud: String,
    pub iat: i64,
    pub exp: i64,
    pub user: Map<String, Value>,
}

/// JWT issuer, intended audience and access token lifetime.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub issuer: String,
    pub audience: String,
    pub access_token_ttl_seconds: u64,
    pub ed25519: ed25519::Config,
}

/// Issues and verifies JWT access tokens.
pub struct TokenService {
    config: Config,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    validation: Validation,
}

impl TokenService {
    pub fn new(config: &Config) -> crate::Result<Self> {
        if config.issuer.is_empty() || config.audience.is_empty() {
            return Err(crate::Error::InvalidParameter(
                "issuer and audience must not be empty".into(),
            ));
        }
        if config.access_token_ttl_seconds == 0 || config.access_token_ttl_seconds > i64::MAX as u64
        {
            return Err(crate::Error::InvalidParameter(
                "access token TTL must be a positive number of seconds".into(),
            ));
        }

        let public_key = std::fs::read(&config.ed25519.public_key_path)?;
        let decoding_key =
            DecodingKey::from_ed_pem(&public_key).context("failed to load Ed25519 public key")?;
        let private_key = std::fs::read(&config.ed25519.key_path)?;
        let encoding_key =
            EncodingKey::from_ed_pem(&private_key).context("failed to load Ed25519 private key")?;

        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_issuer(&[&config.issuer]);
        validation.set_audience(&[&config.audience]);
        validation.set_required_spec_claims(&["sub", "iss", "aud", "exp"]);
        validation.validate_nbf = true;
        validation.leeway = 0;

        let service = Self {
            config: config.clone(),
            encoding_key,
            decoding_key,
            validation,
        };
        let message = b"Ed25519 key pair check";
        let signature =
            jsonwebtoken::crypto::sign(message, &service.encoding_key, Algorithm::EdDSA)
                .context("failed to use Ed25519 private key")?;
        let valid = jsonwebtoken::crypto::verify(
            &signature,
            message,
            &service.decoding_key,
            Algorithm::EdDSA,
        )
        .unwrap_or(false);
        if !valid {
            return Err(crate::Error::InvalidParameter(
                "Ed25519 keys do not form a valid key pair".into(),
            ));
        }
        Ok(service)
    }

    pub fn issue(&self, subject: &str, user: Map<String, Value>) -> crate::Result<String> {
        if subject.is_empty() {
            return Err(crate::Error::InvalidParameter(
                "subject must not be empty".into(),
            ));
        }
        let iat = chrono::Utc::now().timestamp();
        let exp = iat
            .checked_add(self.config.access_token_ttl_seconds as i64)
            .ok_or_else(|| {
                crate::Error::InvalidParameter(
                    "access token expiration exceeds supported range".into(),
                )
            })?;
        let claims = Claims {
            sub: subject.into(),
            iss: self.config.issuer.clone(),
            aud: self.config.audience.clone(),
            iat,
            exp,
            user,
        };
        jsonwebtoken::encode(&Header::new(Algorithm::EdDSA), &claims, &self.encoding_key)
            .context("failed to issue JWT")
            .map_err(crate::Error::from)
    }

    pub fn verify(&self, token: &str) -> crate::Result<Claims> {
        let claims = jsonwebtoken::decode::<Claims>(token, &self.decoding_key, &self.validation)
            .map_err(|_| crate::Error::Validation("invalid or expired JWT".into()))?
            .claims;
        if claims.exp <= chrono::Utc::now().timestamp() || claims.sub.is_empty() {
            return Err(crate::Error::Validation("invalid or expired JWT".into()));
        }
        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issues_and_verifies_access_token() {
        let dir = std::env::temp_dir().join(format!(
            "espr-jwt-token-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let config = Config {
            issuer: "https://auth.example.com".into(),
            audience: "api".into(),
            access_token_ttl_seconds: 900,
            ed25519: ed25519::Config {
                key_path: dir.join("private.pem"),
                public_key_path: dir.join("public.pem"),
            },
        };
        ed25519::generate(&config.ed25519).unwrap();
        let service = TokenService::new(&config).unwrap();
        let user = serde_json::from_value(serde_json::json!({"name": "Kim"})).unwrap();
        let before = chrono::Utc::now().timestamp();

        let token = service.issue("user-1", user).unwrap();
        let claims = service.verify(&token).unwrap();

        let after = chrono::Utc::now().timestamp();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.iss, config.issuer);
        assert_eq!(claims.aud, config.audience);
        assert_eq!(claims.user["name"], "Kim");
        assert!((before..=after).contains(&claims.iat));
        assert_eq!(claims.exp - claims.iat, 900);

        let (header, _) = token.split_once('.').unwrap();
        let (_, signature) = token.rsplit_once('.').unwrap();
        let tampered = format!("{header}.e30.{signature}");
        assert!(matches!(
            service.verify(&tampered),
            Err(crate::Error::Validation(_))
        ));

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn claims_round_trip_with_user_data() {
        let claims = Claims {
            sub: "user-1".into(),
            iss: "https://auth.example.com".into(),
            aud: "api".into(),
            iat: 1_700_000_000,
            exp: 1_700_000_900,
            user: serde_json::from_value(serde_json::json!({"name": "Kim", "roles": ["admin"]}))
                .unwrap(),
        };

        let json = serde_json::to_value(&claims).unwrap();

        assert_eq!(json["sub"], "user-1");
        assert_eq!(json["exp"], 1_700_000_900_i64);
        assert_eq!(json["user"]["name"], "Kim");
        assert_eq!(json["user"]["roles"][0], "admin");
        assert_eq!(serde_json::from_value::<Claims>(json).unwrap(), claims);
    }
}
