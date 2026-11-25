use std::sync::{Arc, RwLock};
use std::time::Duration;

use espr::validate::Validate;
use openidconnect::core::{
    CoreAuthenticationFlow, CoreClient, CoreErrorResponseType, CoreIdToken, CoreIdTokenClaims,
    CoreIdTokenVerifier, CoreJsonWebKeySet, CoreJwsSigningAlgorithm, CoreProviderMetadata,
};
use openidconnect::url::Url;
use openidconnect::{
    AuthType, AuthorizationCode, ClaimsVerificationError, ClientId, ClientSecret, CsrfToken,
    EndpointNotSet, EndpointSet, IssuerUrl, JsonWebKeySetUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, RequestTokenError, Scope, SignatureVerificationError,
    TokenResponse as _, reqwest,
};

use crate::error::ResultExt as _;
use crate::{Code, Error, Result};

const GOOGLE_ISSUER: &str = "https://accounts.google.com";

type GoogleClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointSet,
    EndpointNotSet,
>;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub client_id: String,
    pub client_secret: ClientSecret,
    pub redirect_uri: String,
}

impl Validate for Config {
    type Error = Error;

    fn validate(&self) -> Result<()> { self.validate_and_parse_redirect_uri().map(drop) }
}

impl Config {
    fn validate_and_parse_redirect_uri(&self) -> Result<RedirectUrl> {
        if self.client_id.trim().is_empty() || self.client_secret.secret().trim().is_empty() {
            return Err(Error::new(
                Code::InvalidConfiguration,
                "Google OIDC client ID and secret are required",
            )
            .at(crate::here!()));
        }
        RedirectUrl::new(self.redirect_uri.clone()).map_err(|_| {
            Error::new(
                Code::InvalidConfiguration,
                "Invalid Google OIDC redirect URI",
            )
            .at(crate::here!())
        })
    }
}

pub struct GoogleOidcService {
    client: GoogleClient,
    http: reqwest::Client,
    issuer: IssuerUrl,
    signing_algorithms: Vec<CoreJwsSigningAlgorithm>,
    jwks_uri: JsonWebKeySetUrl,
    verifier: RwLock<Arc<CoreIdTokenVerifier<'static>>>,
}

pub struct LoginRequest {
    pub authorization_url: Url,
    pub state: LoginState,
}

/// Store server-side, bound to the initiating session, with short expiration
/// and single-use removal.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct LoginState {
    csrf_token: CsrfToken,
    nonce: Nonce,
    pkce_verifier: PkceCodeVerifier,
}

impl LoginState {
    pub(crate) fn value(&self) -> &str { self.csrf_token.secret() }
}

pub struct GoogleProfile {
    pub subject: String,
    pub email: String,
    pub email_verified: bool,
    pub nickname: String,
}

impl GoogleProfile {
    fn from_claims(claims: &CoreIdTokenClaims) -> Result<Self> {
        let email = claims
            .email()
            .map(|email| email.as_str().trim())
            .filter(|email| !email.is_empty())
            .ok_or_else(|| {
                Error::new(Code::Unauthorized, "Google profile has no email").at(crate::here!())
            })?;
        let nickname = claims
            .name()
            .and_then(|name| name.get(None))
            .map(|name| name.as_str().trim())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| email.rsplit_once('@').map_or(email, |(local, _)| local));

        Ok(Self {
            subject: claims.subject().as_str().to_owned(),
            email: email.to_owned(),
            email_verified: claims.email_verified().unwrap_or(false),
            nickname: nickname.to_owned(),
        })
    }
}

impl GoogleOidcService {
    pub async fn new(config: &Config) -> Result<Self> {
        let redirect_uri = config.validate_and_parse_redirect_uri()?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .map_err_internal(
                "Failed to initialize Google OIDC HTTP client",
                crate::here!(),
            )?;
        let issuer = IssuerUrl::new(GOOGLE_ISSUER.to_owned()).expect("valid Google issuer URL");
        let metadata = CoreProviderMetadata::discover_async(issuer.clone(), &http)
            .await
            .map_err_internal("Failed to discover Google OIDC provider", crate::here!())?;
        let token_endpoint = metadata.token_endpoint().cloned().ok_or_else(|| {
            Error::internal("Google OIDC discovery is missing the token endpoint")
                .at(crate::here!())
        })?;
        let jwks_uri = metadata.jwks_uri().clone();
        let signing_algorithms = metadata.id_token_signing_alg_values_supported().to_vec();
        let client_id = ClientId::new(config.client_id.clone());
        let verifier = Self::token_verifier(
            &client_id,
            &issuer,
            &signing_algorithms,
            metadata.jwks().clone(),
        );
        let client = CoreClient::new(client_id, issuer.clone(), CoreJsonWebKeySet::default())
            .set_auth_uri(metadata.authorization_endpoint().clone())
            .set_client_secret(config.client_secret.clone())
            .set_auth_type(AuthType::RequestBody)
            .set_token_uri(token_endpoint)
            .set_redirect_uri(redirect_uri);

        Ok(Self {
            client,
            http,
            issuer,
            signing_algorithms,
            jwks_uri,
            verifier: RwLock::new(Arc::new(verifier)),
        })
    }

    pub fn begin_login(&self) -> LoginRequest {
        let (challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let (authorization_url, csrf_token, nonce) = self
            .client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .add_scope(Scope::new("email".into()))
            .add_scope(Scope::new("profile".into()))
            .set_pkce_challenge(challenge)
            .url();

        LoginRequest {
            authorization_url,
            state: LoginState {
                csrf_token,
                nonce,
                pkce_verifier,
            },
        }
    }

    pub async fn complete_login(
        &self,
        code: String,
        callback_state: String,
        state: LoginState,
    ) -> Result<GoogleProfile> {
        if CsrfToken::new(callback_state) != state.csrf_token {
            return Err(
                Error::new(Code::Unauthorized, "Invalid Google login state").at(crate::here!())
            );
        }
        let tokens =
            self.client
                .exchange_code(AuthorizationCode::new(code))
                .set_pkce_verifier(state.pkce_verifier)
                .request_async(&self.http)
                .await
                .map_err(|error| match error {
                    RequestTokenError::ServerResponse(response)
                        if *response.error() == CoreErrorResponseType::InvalidGrant =>
                    {
                        Error::new(Code::Unauthorized, "Invalid Google authorization code")
                            .at(crate::here!())
                    }
                    _ => Error::internal("Google authorization code exchange failed")
                        .at(crate::here!()),
                })?;
        let id_token = tokens.id_token().ok_or_else(|| {
            Error::new(Code::Unauthorized, "Google response has no ID token").at(crate::here!())
        })?;
        let claims = self.verify_claims(id_token, &state.nonce).await?;
        GoogleProfile::from_claims(claims)
    }

    async fn verify_claims<'a>(
        &self,
        id_token: &'a CoreIdToken,
        nonce: &Nonce,
    ) -> Result<&'a CoreIdTokenClaims> {
        let cached = Arc::clone(&self.verifier.read().unwrap());
        let claims = match id_token.claims(&cached, nonce) {
            Err(ClaimsVerificationError::SignatureVerification(
                SignatureVerificationError::NoMatchingKey,
            )) => {
                let keys = CoreJsonWebKeySet::fetch_async(&self.jwks_uri, &self.http)
                    .await
                    .map_err_internal(
                        "Failed to refresh Google OIDC public keys",
                        crate::here!(),
                    )?;
                let fresh = Self::token_verifier(
                    self.client.client_id(),
                    &self.issuer,
                    &self.signing_algorithms,
                    keys,
                );
                let claims = id_token.claims(&fresh, nonce);
                let mut cached_now = self.verifier.write().unwrap();
                // Keep the verifier published by a concurrent refresh.
                if Arc::ptr_eq(&cached_now, &cached) {
                    *cached_now = Arc::new(fresh);
                }
                claims
            }
            result => result,
        };
        claims.map_err(|_| {
            Error::new(Code::Unauthorized, "Invalid Google ID token").at(crate::here!())
        })
    }

    fn token_verifier(
        client_id: &ClientId,
        issuer: &IssuerUrl,
        signing_algorithms: &[CoreJwsSigningAlgorithm],
        keys: CoreJsonWebKeySet,
    ) -> CoreIdTokenVerifier<'static> {
        CoreIdTokenVerifier::new_public_client(client_id.clone(), issuer.clone(), keys)
            .set_allowed_algs(signing_algorithms.iter().cloned())
    }
}

#[cfg(test)]
pub(crate) use tests::service_with_key as test_service_with_key;

#[cfg(test)]
mod tests {
    use openidconnect::{AuthUrl, TokenUrl};

    use super::*;

    pub(crate) fn service(
        jwks_uri: &str,
        signing_algorithms: Vec<CoreJwsSigningAlgorithm>,
    ) -> GoogleOidcService {
        let issuer = IssuerUrl::new(GOOGLE_ISSUER.into()).unwrap();
        let client_id = ClientId::new("example-client".into());
        let verifier = GoogleOidcService::token_verifier(
            &client_id,
            &issuer,
            &signing_algorithms,
            CoreJsonWebKeySet::default(),
        );
        let client = CoreClient::new(client_id, issuer.clone(), CoreJsonWebKeySet::default())
            .set_auth_uri(AuthUrl::new("https://example.com/authorize".into()).unwrap())
            .set_token_uri(TokenUrl::new("http://127.0.0.1:0/token".into()).unwrap())
            .set_redirect_uri(
                RedirectUrl::new("https://example.com/login/google/callback".into()).unwrap(),
            );

        GoogleOidcService {
            client,
            http: reqwest::Client::builder().no_proxy().build().unwrap(),
            issuer,
            signing_algorithms,
            jwks_uri: JsonWebKeySetUrl::new(jwks_uri.into()).unwrap(),
            verifier: RwLock::new(Arc::new(verifier)),
        }
    }

    pub(crate) fn service_with_key(token_url: &str, keys: CoreJsonWebKeySet) -> GoogleOidcService {
        let mut service = service(
            "http://127.0.0.1:0/jwks",
            vec![CoreJwsSigningAlgorithm::EdDsa],
        );
        service.verifier = RwLock::new(Arc::new(GoogleOidcService::token_verifier(
            service.client.client_id(),
            &service.issuer,
            &service.signing_algorithms,
            keys,
        )));
        service.client = service
            .client
            .set_token_uri(TokenUrl::new(token_url.into()).unwrap());
        service
    }

    mod contract {
        use super::*;

        #[test]
        fn uses_google_name_when_present() {
            let claims = profile_claims(Some("nickname@example.com"), Some("Google Name"));
            let profile = GoogleProfile::from_claims(&claims).unwrap();

            // Google 이름을 닉네임으로 사용하고 이메일과 인증 여부를 전달한다.
            assert_eq!(profile.nickname, "Google Name");
            assert_eq!(profile.email, "nickname@example.com");
            assert!(profile.email_verified);
        }

        #[test]
        fn uses_email_local_part_when_name_is_missing() {
            let claims = profile_claims(Some("nickname@example.com"), None);
            let profile = GoogleProfile::from_claims(&claims).unwrap();

            // 이름이 없으면 이메일의 @ 앞부분이 닉네임이다.
            assert_eq!(profile.nickname, "nickname");
        }

        #[test]
        fn rejects_profile_without_email() {
            let claims = profile_claims(None, Some("Google Name"));

            // 이름이 있어도 이메일이 없으면 인증 오류를 반환한다.
            assert!(matches!(
                GoogleProfile::from_claims(&claims),
                Err(error) if error.code == Code::Unauthorized
            ));
        }

        fn profile_claims(email: Option<&str>, name: Option<&str>) -> CoreIdTokenClaims {
            serde_json::from_value(serde_json::json!({
                "iss": GOOGLE_ISSUER,
                "aud": "example-client",
                "sub": "google-user",
                "iat": 1_700_000_000,
                "exp": 1_700_003_600,
                "email": email,
                "email_verified": true,
                "name": name
            }))
            .unwrap()
        }

        #[test]
        fn validates_credentials_and_redirect_uri() {
            let valid = Config {
                client_id: "example-client".into(),
                client_secret: ClientSecret::new("example-secret".into()),
                redirect_uri: "https://example.com/callback".into(),
            };

            // 자격 증명과 리다이렉트 URI가 유효하면 설정을 허용한다.
            assert!(valid.validate().is_ok());

            let invalid_uri = Config {
                redirect_uri: "invalid".into(),
                ..valid.clone()
            };

            // 파싱할 수 없는 리다이렉트 URI는 설정 오류다.
            assert!(matches!(
                invalid_uri.validate(),
                Err(error) if error.code == Code::InvalidConfiguration
            ));

            let blank_secret = Config {
                client_secret: ClientSecret::new(" ".into()),
                ..valid
            };

            // 자격 증명이 공백으로만 구성되면 설정 오류다.
            assert!(matches!(
                blank_secret.validate(),
                Err(error) if error.code == Code::InvalidConfiguration
            ));
        }

        #[tokio::test]
        async fn rejects_mismatched_callback_state_before_token_exchange() {
            let service = service("https://example.com/jwks", vec![]);
            let login = service.begin_login();
            let request_state = login
                .authorization_url
                .query_pairs()
                .find(|(key, _)| key == "state")
                .unwrap()
                .1;

            // 인가 요청의 state와 콜백에서 대조할 state는 동일하다.
            assert_eq!(request_state, login.state.csrf_token.secret().as_str());

            let result = service
                .complete_login("code".into(), "unrelated-state".into(), login.state)
                .await;

            // state가 다르면 코드를 교환하기 전에 인증 오류를 반환한다.
            assert!(matches!(
                result,
                Err(error)
                    if error.code == Code::Unauthorized && error.message == "Invalid Google login state"
            ));
        }
    }

    mod edge {
        use axum::routing::get;
        use axum::{Json, Router};
        use openidconnect::core::CoreEdDsaPrivateSigningKey;
        use openidconnect::{JsonWebKeyId, PrivateSigningKey};
        use tokio::net::TcpListener;

        use super::*;

        #[tokio::test]
        async fn verifies_id_token_with_rotated_key_and_reuses_it() {
            // 테스트 토큰 서명에만 사용하는 키.
            let signing_key = CoreEdDsaPrivateSigningKey::from_ed25519_pem(
                "-----BEGIN PRIVATE KEY-----
MC4CAQAwBQYDK2VwBCIEID9fvP22Y4BWeCEB8aYApFGjC0filpMDrYsHXz70LRJG
-----END PRIVATE KEY-----",
                Some(JsonWebKeyId::new("rotated-key".into())),
            )
            .unwrap();
            let keys = CoreJsonWebKeySet::new(vec![signing_key.as_verification_key()]);
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let jwks_uri = format!("http://{}/jwks", listener.local_addr().unwrap());
            let app = Router::new().route("/jwks", get(move || async { Json(keys) }));
            let server = tokio::spawn(async move { axum::serve(listener, app).await });

            let algorithm = CoreJwsSigningAlgorithm::EdDsa;
            let service = service(&jwks_uri, vec![algorithm.clone()]);
            let nonce = Nonce::new("login-nonce".into());
            let now = jiff::Timestamp::now().as_second();
            let claims = serde_json::from_value(serde_json::json!({
                "iss": GOOGLE_ISSUER,
                "aud": "example-client",
                "sub": "google-user",
                "iat": now,
                "exp": now + 300,
                "nonce": nonce.secret()
            }))
            .unwrap();
            let token = CoreIdToken::new(claims, &signing_key, algorithm, None, None).unwrap();

            let refreshed = service.verify_claims(&token, &nonce).await;
            server.abort();
            let _ = server.await;

            // 기존 검증기에 없는 키를 조회해 같은 로그인 요청을 검증한다.
            assert_eq!(refreshed.unwrap().subject().as_str(), "google-user");

            let cached = service.verify_claims(&token, &nonce).await;

            // JWKS 엔드포인트가 내려간 뒤에도 갱신한 키로 검증할 수 있다.
            assert_eq!(cached.unwrap().subject().as_str(), "google-user");
        }
    }
}
