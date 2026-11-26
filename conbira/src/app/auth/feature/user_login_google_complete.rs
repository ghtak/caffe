pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::auth::service::login_tokens::LoginTokens;
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::ApiResult;
    use crate::{Code, Error};

    #[derive(serde::Deserialize)]
    pub struct RequestBody {
        pub code: String,
        pub state: String,
        pub browser_proof: String,
    }

    impl espr::validate::Validate for RequestBody {
        type Error = Error;
        fn validate(&self) -> crate::Result<()> {
            if self.code.trim().is_empty()
                || self.state.trim().is_empty()
                || self.browser_proof.trim().is_empty()
            {
                return Err(Error::new(
                    Code::InvalidRequest,
                    "Code, state and browser proof are required",
                )
                .at(crate::here!()));
            }
            Ok(())
        }
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiJson(request): ApiJson<RequestBody>,
    ) -> ApiResult<LoginTokens> {
        let oidc = container.google_oidc_service.as_ref().ok_or_else(|| {
            Error::new(Code::NotFound, "Google login is disabled").at(crate::here!())
        })?;
        let output = super::usecase::execute(
            &container,
            oidc,
            request.code,
            request.state,
            request.browser_proof,
        )
        .await?;
        Ok(output.into())
    }
}

pub mod usecase {
    use super::super::user_login_google::usecase::state_key;
    use crate::app::auth::entity::{Provider, UserStatus};
    use crate::app::auth::repository::{identity, user};
    use crate::app::auth::service::google_oidc::GoogleOidcService;
    use crate::app::auth::service::login_tokens::{self, LoginTokens};
    use crate::app::rbac::repository::{role, user_role};
    use crate::app::{Container, DatabaseExt as _};
    use crate::error::ResultExt as _;
    use crate::{Code, Error};

    pub(crate) async fn execute(
        container: &Container,
        oidc: &GoogleOidcService,
        code: String,
        state: String,
        browser_proof: String,
    ) -> crate::Result<LoginTokens> {
        let key = state_key(&state, &browser_proof);
        let value: String = container
            .cache
            .take(key)
            .await
            .map_err(|_| {
                Error::internal("Failed to consume Google login state").at(crate::here!())
            })?
            .ok_or_else(|| {
                Error::new(Code::Unauthorized, "Invalid Google login state").at(crate::here!())
            })?;
        let login = serde_json::from_str(&value).expect("valid cached Google login state");
        let profile = oidc.complete_login(code, state, login).await?;
        let mut tx = container.db.begin_tx(crate::here!()).await?;
        let mut handle = tx.handle();
        identity::lock_subject(&mut handle, &profile.subject).await?;
        let identity = match identity::find_by_provider_subject(
            &mut handle,
            Provider::Google,
            &profile.subject,
        )
        .await?
        {
            Some(identity) => identity,
            None => {
                if !profile.email_verified {
                    return Err(
                        Error::new(Code::Unauthorized, "Google email is not verified")
                            .at(crate::here!()),
                    );
                }
                let user = user::insert(
                    &mut handle,
                    &user::NewUser {
                        nickname: profile.nickname,
                        status: UserStatus::Registered,
                    },
                )
                .await?;
                let identity = identity::insert(
                    &mut handle,
                    &identity::NewIdentity {
                        user_id: user.id,
                        provider: Provider::Google,
                        provider_subject: profile.subject,
                        email: profile.email.trim().to_ascii_lowercase(),
                        password_hash: None,
                    },
                )
                .await?;
                let role = role::find_by_code(&mut handle, "user")
                    .await?
                    .ok_or_else(|| Error::internal("User role not found").at(crate::here!()))?;
                user_role::insert(&mut handle, user.id, role.id).await?;
                identity
            }
        };
        tx.commit()
            .await
            .map_err_internal("Failed to commit Google login", crate::here!())?;
        login_tokens::issue(
            &container.db,
            &container.jwt_token_service,
            &container.refresh_token_service,
            identity.user_id,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    mod contract {
        use std::collections::HashMap;
        use std::sync::Arc;
        use std::time::Duration;

        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode, header};
        use axum::{Form, Json, Router};
        use openidconnect::core::{
            CoreEdDsaPrivateSigningKey, CoreIdToken, CoreJsonWebKeySet, CoreJwsSigningAlgorithm,
        };
        use openidconnect::{JsonWebKeyId, PrivateSigningKey};
        use serde_json::{Value, json};
        use tower::ServiceExt as _;

        use crate::app::auth::feature::user_login_google::usecase::state_key;
        use crate::app::auth::service::google_oidc::test_service_with_key;

        struct Fixture {
            signing_key: CoreEdDsaPrivateSigningKey,
            server: tokio::task::JoinHandle<std::io::Result<()>>,
            container: Arc<crate::app::Container>,
            router: Router,
        }

        impl Fixture {
            async fn new(context: &crate::test_support::TestContext) -> anyhow::Result<Self> {
                let pem = concat!(
                    "-----BEGIN PRIVATE KEY-----\n",
                    "MC4CAQAwBQYDK2VwBCIEID9fvP22Y4BWeCEB8aYApFGjC0filpMDrYsHXz70LRJG\n",
                    "-----END PRIVATE KEY-----",
                );
                let signing_key = CoreEdDsaPrivateSigningKey::from_ed25519_pem(
                    pem,
                    Some(JsonWebKeyId::new("test-key".into())),
                )
                .expect("valid test signing key");
                let keys = CoreJsonWebKeySet::new(vec![signing_key.as_verification_key()]);
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
                let token_url = format!("http://{}/token", listener.local_addr()?);
                let mock = Router::new().route("/token", axum::routing::post(exchange_token));
                let mut container = context.container().await?;
                container.google_oidc_service = Some(test_service_with_key(&token_url, keys));
                let container = Arc::new(container);
                let router = crate::app::http::router(container.clone())?;
                let server = tokio::spawn(async move { axum::serve(listener, mock).await });
                Ok(Fixture {
                    signing_key,
                    server,
                    container,
                    router,
                })
            }

            async fn start(&self) -> anyhow::Result<Value> {
                let response = self
                    .router
                    .clone()
                    .oneshot(Request::post("/api/auth/login/google").body(Body::empty())?)
                    .await?;
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
                assert!(!response.headers().contains_key(header::SET_COOKIE));
                Ok(
                    serde_json::from_slice::<Value>(&to_bytes(response.into_body(), 16384).await?)?
                        ["data"]
                        .clone(),
                )
            }

            async fn complete(
                &self,
                data: &Value,
                code: &str,
            ) -> anyhow::Result<axum::response::Response> {
                let request = Request::post("/api/auth/login/google/complete")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "state": data["state"],
                            "browser_proof": data["browser_proof"],
                            "code": code,
                        })
                        .to_string(),
                    ))?;
                Ok(self.router.clone().oneshot(request).await?)
            }

            fn signed_code(
                &self,
                data: &Value,
                subject: &str,
                verified: bool,
            ) -> anyhow::Result<String> {
                let url =
                    openidconnect::url::Url::parse(data["authorization_url"].as_str().unwrap())?;
                let nonce = url
                    .query_pairs()
                    .find_map(|(k, v)| (k == "nonce").then(|| v.into_owned()))
                    .unwrap();
                let now = jiff::Timestamp::now().as_second();
                let claims = serde_json::from_value(json!({
                    "iss": "https://accounts.google.com",
                    "aud": "example-client",
                    "sub": subject,
                    "iat": now,
                    "exp": now + 300,
                    "nonce": nonce,
                    "email": "same@example.com",
                    "email_verified": verified,
                    "name": "Google User",
                }))?;
                Ok(CoreIdToken::new(
                    claims,
                    &self.signing_key,
                    CoreJwsSigningAlgorithm::EdDsa,
                    None,
                    None,
                )?
                .to_string())
            }
        }

        async fn exchange_token(Form(params): Form<HashMap<String, String>>) -> Json<Value> {
            Json(json!({
                "access_token": "mock-access",
                "token_type": "Bearer",
                "id_token": params.get("code").expect("authorization code"),
            }))
        }

        impl Drop for Fixture {
            fn drop(&mut self) { self.server.abort(); }
        }

        #[tokio::test]
        async fn login_start_binds_state_and_browser_proof() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(4, |context| async move {
                let fixture = Fixture::new(&context).await?;
                // 로그인 URL의 state와 브라우저 증명을 바인딩한다.
                let first = fixture.start().await?;
                let authorization =
                    openidconnect::url::Url::parse(first["authorization_url"].as_str().unwrap())?;
                let params: std::collections::HashMap<_, _> =
                    authorization.query_pairs().into_owned().collect();
                assert_eq!(
                    params.get("redirect_uri").map(String::as_str),
                    Some("https://example.com/login/google/callback")
                );
                assert_eq!(
                    params.get("state").map(String::as_str),
                    first["state"].as_str()
                );
                assert!(
                    !authorization
                        .as_str()
                        .contains(first["browser_proof"].as_str().unwrap())
                );
                assert_ne!(first["state"], first["browser_proof"]);
                let code = fixture.signed_code(&first, "subject", true)?;
                let mut wrong = first.clone();
                wrong["browser_proof"] = json!("wrong-proof");
                assert_eq!(
                    fixture.complete(&wrong, &code).await?.status(),
                    StatusCode::UNAUTHORIZED
                );
                let mut shifted = first.clone();
                let proof = first["browser_proof"].as_str().unwrap();
                shifted["state"] = json!(format!(
                    "{}{}",
                    first["state"].as_str().unwrap(),
                    &proof[..1]
                ));
                shifted["browser_proof"] = json!(&proof[1..]);
                assert_eq!(
                    fixture.complete(&shifted, &code).await?.status(),
                    StatusCode::UNAUTHORIZED
                );
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn concurrent_logins_create_one_account_and_issue_tokens() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(4, |context| async move {
                let fixture = Fixture::new(&context).await?;
                let mut conn = fixture.container.db.conn().await?;
                sqlx::query("INSERT INTO conbira_auth_users (nickname) VALUES ('local')")
                    .execute(conn.handle().conn())
                    .await?;
                sqlx::query(
                    "INSERT INTO conbira_auth_user_identities
                        (user_id, provider, provider_subject, email, password_hash)
                     SELECT id, 'local', id::text, 'same@example.com', 'hash'
                     FROM conbira_auth_users WHERE nickname = 'local'",
                )
                .execute(conn.handle().conn())
                .await?;
                drop(conn);
                let first = fixture.start().await?;
                let code = fixture.signed_code(&first, "subject", true)?;
                // 동일 Google 계정의 동시 로그인은 하나의 계정으로 토큰을
                // 발급한다.
                let second = fixture.start().await?;
                let second_code = fixture.signed_code(&second, "subject", true)?;
                let (a, b) = tokio::join!(
                    fixture.complete(&first, &code),
                    fixture.complete(&second, &second_code)
                );
                let a = a?;
                let b = b?;
                assert_eq!(a.status(), StatusCode::OK);
                assert_eq!(b.status(), StatusCode::OK);
                assert_eq!(a.headers()[header::CACHE_CONTROL], "no-store");
                assert!(!a.headers().contains_key(header::SET_COOKIE));
                let first_body: Value =
                    serde_json::from_slice(&to_bytes(a.into_body(), 16384).await?)?;
                let second_body: Value =
                    serde_json::from_slice(&to_bytes(b.into_body(), 16384).await?)?;
                let google_id = first_body["data"]["claims"]["id"].as_i64().unwrap();
                assert_eq!(second_body["data"]["claims"]["id"], google_id);
                assert_eq!(first_body["data"]["claims"]["status"], "registered");
                assert!(first_body["data"]["refresh_token"].as_str().is_some());
                let mut conn = fixture.container.db.conn().await?;
                let rows: Vec<(i64, String, String)> = sqlx::query_as(
                    "SELECT user_id, provider, email
                 FROM conbira_auth_user_identities ORDER BY provider",
                )
                .fetch_all(conn.handle().conn())
                .await?;
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0].1, "google");
                assert_eq!(rows[0].0, google_id);
                assert_ne!(rows[0].0, rows[1].0);
                assert_eq!(rows[0].2, rows[1].2);
                drop(conn);
                let token = first_body["data"]["access_token"].as_str().unwrap();
                let me = fixture
                    .router
                    .clone()
                    .oneshot(
                        Request::get("/api/auth/me")
                            .header(header::AUTHORIZATION, format!("Bearer {token}"))
                            .body(Body::empty())?,
                    )
                    .await?;
                assert_eq!(me.status(), StatusCode::OK);
                let me: Value = serde_json::from_slice(&to_bytes(me.into_body(), 16384).await?)?;
                assert_eq!(me["data"]["sub"], google_id.to_string());
                assert!(me["data"]["user"].get("roles").is_none());
                assert_eq!(
                    fixture.complete(&first, &code).await?.status(),
                    StatusCode::UNAUTHORIZED
                );
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn existing_identity_allows_unverified_email() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(4, |context| async move {
                let fixture = Fixture::new(&context).await?;
                let first = fixture.start().await?;
                let code = fixture.signed_code(&first, "subject", true)?;
                assert_eq!(
                    fixture.complete(&first, &code).await?.status(),
                    StatusCode::OK
                );
                // 기존 식별 정보로 로그인할 때 이메일 재검증을 요구하지 않는다.
                let repeated = fixture.start().await?;
                let unverified_code = fixture.signed_code(&repeated, "subject", false)?;
                assert_eq!(
                    fixture
                        .complete(&repeated, &unverified_code)
                        .await?
                        .status(),
                    StatusCode::OK
                );
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn login_state_is_consumed_once_concurrently() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(4, |context| async move {
                let fixture = Fixture::new(&context).await?;
                // 같은 로그인 상태를 동시에 사용해도 한 요청만 성공한다.
                let once = fixture.start().await?;
                let once_code = fixture.signed_code(&once, "subject", true)?;
                let (a, b) = tokio::join!(
                    fixture.complete(&once, &once_code),
                    fixture.complete(&once, &once_code)
                );
                let mut statuses = [a?.status(), b?.status()];
                statuses.sort();
                assert_eq!(statuses, [StatusCode::OK, StatusCode::UNAUTHORIZED]);
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn new_identity_requires_verified_email() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(4, |context| async move {
                let fixture = Fixture::new(&context).await?;
                // 새 계정은 검증된 이메일이 있어야 로그인할 수 있다.
                let unverified = fixture.start().await?;
                let unverified_code = fixture.signed_code(&unverified, "new-subject", false)?;
                assert_eq!(
                    fixture
                        .complete(&unverified, &unverified_code)
                        .await?
                        .status(),
                    StatusCode::UNAUTHORIZED
                );
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn inactive_and_withdrawn_users_cannot_log_in() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(4, |context| async move {
                let fixture = Fixture::new(&context).await?;
                let first = fixture.start().await?;
                let code = fixture.signed_code(&first, "subject", true)?;
                let response = fixture.complete(&first, &code).await?;
                let body: Value =
                    serde_json::from_slice(&to_bytes(response.into_body(), 16384).await?)?;
                let google_id = body["data"]["claims"]["id"].as_i64().unwrap();
                // 비활성·탈퇴 계정은 로그인할 수 없다.
                for status in ["inactive", "withdrawn"] {
                    let mut conn = fixture.container.db.conn().await?;
                    sqlx::query("UPDATE conbira_auth_users SET status = $1 WHERE id = $2")
                        .bind(status)
                        .bind(google_id)
                        .execute(conn.handle().conn())
                        .await?;
                    drop(conn);
                    let data = fixture.start().await?;
                    let code = fixture.signed_code(&data, "subject", true)?;
                    assert_eq!(
                        fixture.complete(&data, &code).await?.status(),
                        StatusCode::UNAUTHORIZED
                    );
                }
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn expired_login_state_is_rejected() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(4, |context| async move {
                let fixture = Fixture::new(&context).await?;
                // 만료된 로그인 상태는 거절한다.
                let expired = fixture.start().await?;
                let key = state_key(
                    expired["state"].as_str().unwrap(),
                    expired["browser_proof"].as_str().unwrap(),
                );
                let value: String = fixture.container.cache.take(&key).await?.unwrap();
                fixture
                    .container
                    .cache
                    .set_with_ttl(&key, value, Duration::from_millis(1))
                    .await?;
                tokio::time::sleep(Duration::from_millis(10)).await;
                assert_eq!(
                    fixture.complete(&expired, "code").await?.status(),
                    StatusCode::UNAUTHORIZED
                );
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn missing_required_inputs_are_rejected() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(4, |context| async move {
                let fixture = Fixture::new(&context).await?;
                // 기대 결과: 필수 입력이 없거나 비어 있으면 요청을 거절한다
                for body in ["{}", r#"{"code":"","state":"s","browser_proof":"p"}"#] {
                    let request = Request::post("/api/auth/login/google/complete")
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(body))?;
                    assert_eq!(
                        fixture.router.clone().oneshot(request).await?.status(),
                        StatusCode::BAD_REQUEST
                    );
                }
                Ok(())
            })
            .await
        }
    }
}
