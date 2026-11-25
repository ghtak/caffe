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

    pub(super) async fn execute(
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
        use std::future::Future;
        use std::sync::Arc;
        use std::time::Duration;

        use anyhow::Context as _;
        use axum::body::{Body, Bytes, to_bytes};
        use axum::http::{Request, StatusCode, header};
        use axum::{Json, Router};
        use openidconnect::core::{
            CoreEdDsaPrivateSigningKey, CoreIdToken, CoreJsonWebKeySet, CoreJwsSigningAlgorithm,
        };
        use openidconnect::{JsonWebKeyId, PrivateSigningKey};
        use serde_json::{Value, json};
        use tower::ServiceExt as _;

        use crate::app::auth::feature::user_login_google::usecase::state_key;
        use crate::app::auth::service::google_oidc::test_service_with_key;
        use crate::app::container::db::Database;

        async fn start(router: &Router) -> anyhow::Result<Value> {
            let response = router
                .clone()
                .oneshot(Request::post("/api/auth/login/google").body(Body::empty())?)
                .await?;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            assert!(!response.headers().contains_key(header::SET_COOKIE));
            Ok(serde_json::from_slice::<Value>(&to_bytes(response.into_body(), 16384).await?)?["data"].clone())
        }

        fn completion(data: &Value, code: &str) -> Request<Body> {
            Request::post("/api/auth/login/google/complete").header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"state":data["state"], "browser_proof":data["browser_proof"], "code":code}).to_string())).unwrap()
        }

        fn signed_code(
            data: &Value,
            subject: &str,
            verified: bool,
            key: &CoreEdDsaPrivateSigningKey,
        ) -> anyhow::Result<String> {
            let url = openidconnect::url::Url::parse(data["authorization_url"].as_str().unwrap())?;
            let nonce = url
                .query_pairs()
                .find_map(|(k, v)| (k == "nonce").then(|| v.into_owned()))
                .unwrap();
            let now = jiff::Timestamp::now().as_second();
            let claims = serde_json::from_value(
                json!({"iss":"https://accounts.google.com", "aud":"example-client", "sub":subject,
                "iat":now, "exp":now+300, "nonce":nonce, "email":"same@example.com", "email_verified":verified, "name":"Google User"}),
            )?;
            Ok(
                CoreIdToken::new(claims, key, CoreJwsSigningAlgorithm::EdDsa, None, None)?
                    .to_string(),
            )
        }

        struct Fixture {
            signing_key: CoreEdDsaPrivateSigningKey,
            server: tokio::task::JoinHandle<std::io::Result<()>>,
            container: Arc<crate::app::Container>,
            router: Router,
        }

        impl Fixture {
            async fn run<F, Fut>(test: F) -> anyhow::Result<()>
            where
                F: FnOnce(Self) -> Fut + Send + 'static,
                Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
            {
                let schema = format!("google_test_{}", uuid::Uuid::new_v4().simple());
                let mut config = crate::test_support::database_config(4);
                let admin = Database::new(&config).await?;
                let mut url = openidconnect::url::Url::parse(&config.url)?;
                url.query_pairs_mut()
                    .append_pair("options", &format!("-csearch_path={schema}"));
                config.url = url.to_string();

                let mut conn = admin.conn().await?;
                sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
                    .execute(conn.handle().conn())
                    .await?;

                // Keep setup and test panics in the child task so teardown can
                // run first.
                let result = tokio::spawn(async move {
                    let fixture = Self::new(Database::new(&config).await?).await?;
                    test(fixture).await
                })
                .await;
                let cleanup =
                    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
                        .execute(conn.handle().conn())
                        .await
                        .context("failed to drop Google login test schema");

                let result = match result {
                    Ok(result) => result,
                    Err(error) if error.is_panic() => {
                        if let Err(error) = cleanup {
                            eprintln!("{error:#}");
                        }
                        std::panic::resume_unwind(error.into_panic());
                    }
                    Err(error) => Err(error.into()),
                };
                match (result, cleanup) {
                    (Err(error), Err(cleanup)) => Err(error.context(format!("{cleanup:#}"))),
                    (Err(error), _) => Err(error),
                    (Ok(()), cleanup) => cleanup.map(|_| ()),
                }
            }

            async fn new(db: Database) -> anyhow::Result<Self> {
                let signing_key = CoreEdDsaPrivateSigningKey::from_ed25519_pem(
                "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID9fvP22Y4BWeCEB8aYApFGjC0filpMDrYsHXz70LRJG\n-----END PRIVATE KEY-----",
                Some(JsonWebKeyId::new("test-key".into())),
            ).expect("valid test signing key");
                let keys = CoreJsonWebKeySet::new(vec![signing_key.as_verification_key()]);
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
                let token_url = format!("http://{}/token", listener.local_addr()?);
                let mock = Router::new().route("/token", axum::routing::post(|body: Bytes| async move {
                let code = openidconnect::url::form_urlencoded::parse(&body).find_map(|(k,v)| (k == "code").then(|| v.into_owned())).unwrap();
                Json(json!({"access_token":"mock-access", "token_type":"Bearer", "id_token":code}))
            }));
                let mut container = crate::test_support::container().await?;
                Arc::get_mut(&mut container).unwrap().db = db;
                Arc::get_mut(&mut container).unwrap().google_oidc_service =
                    Some(test_service_with_key(&token_url, keys));
                let mut conn = container.db.conn().await?;
                for sql in [
                    include_str!("../../../../migrations/0010_user.sql"),
                    include_str!("../../../../migrations/0011_refresh_token.sql"),
                    include_str!("../../../../migrations/0012_rbac.sql"),
                ] {
                    sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
                        .execute(conn.handle().conn())
                        .await?;
                }
                sqlx::query("INSERT INTO conbira_auth_users (nickname) VALUES ('local')")
                    .execute(conn.handle().conn())
                    .await?;
                sqlx::query("INSERT INTO conbira_auth_user_identities (user_id, provider, provider_subject, email, password_hash) SELECT id, 'local', id::text, 'same@example.com', 'hash' FROM conbira_auth_users WHERE nickname = 'local'").execute(conn.handle().conn()).await?;
                drop(conn);
                let router = crate::app::http::router(container.clone())?;
                let server = tokio::spawn(async move { axum::serve(listener, mock).await });
                Ok(Fixture {
                    signing_key,
                    server,
                    container,
                    router,
                })
            }
        }

        impl Drop for Fixture {
            fn drop(&mut self) { self.server.abort(); }
        }

        #[tokio::test]
        async fn login_start_binds_state_and_browser_proof() -> anyhow::Result<()> {
            Fixture::run(|fixture| async move {
                let router = &fixture.router;
                let signing_key = &fixture.signing_key;
                // 기대 결과: login start binds state and browser proof
                let first = start(router).await?;
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
                let code = signed_code(&first, "subject", true, signing_key)?;
                let mut wrong = first.clone();
                wrong["browser_proof"] = json!("wrong-proof");
                assert_eq!(
                    router
                        .clone()
                        .oneshot(completion(&wrong, &code))
                        .await?
                        .status(),
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
                    router
                        .clone()
                        .oneshot(completion(&shifted, &code))
                        .await?
                        .status(),
                    StatusCode::UNAUTHORIZED
                );
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn concurrent_logins_create_one_account_and_issue_tokens() -> anyhow::Result<()> {
            Fixture::run(|fixture| async move {
            let router = &fixture.router;
            let container = &fixture.container;
            let signing_key = &fixture.signing_key;
            let first = start(router).await?;
            let code = signed_code(&first, "subject", true, &fixture.signing_key)?;
            // 기대 결과: concurrent logins create one account and issue tokens
            let second = start(router).await?;
            let second_code = signed_code(&second, "subject", true, signing_key)?;
            let (a, b) = tokio::join!(
                router.clone().oneshot(completion(&first, &code)),
                router.clone().oneshot(completion(&second, &second_code))
            );
            let a = a?;
            let b = b?;
            assert_eq!(a.status(), StatusCode::OK);
            assert_eq!(b.status(), StatusCode::OK);
            assert_eq!(a.headers()[header::CACHE_CONTROL], "no-store");
            assert!(!a.headers().contains_key(header::SET_COOKIE));
            let first_body: Value = serde_json::from_slice(&to_bytes(a.into_body(), 16384).await?)?;
            let second_body: Value =
                serde_json::from_slice(&to_bytes(b.into_body(), 16384).await?)?;
            let google_id = first_body["data"]["claims"]["id"].as_i64().unwrap();
            assert_eq!(second_body["data"]["claims"]["id"], google_id);
            assert_eq!(first_body["data"]["claims"]["status"], "registered");
            assert!(first_body["data"]["refresh_token"].as_str().is_some());
            let mut conn = container.db.conn().await?;
            let rows: Vec<(i64, String, String)> = sqlx::query_as("SELECT user_id, provider, email FROM conbira_auth_user_identities ORDER BY provider").fetch_all(conn.handle().conn()).await?;
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].1, "google");
            assert_eq!(rows[0].0, google_id);
            assert_ne!(rows[0].0, rows[1].0);
            assert_eq!(rows[0].2, rows[1].2);
            drop(conn);
            let token = first_body["data"]["access_token"].as_str().unwrap();
            let me = router
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
            assert_eq!(me["data"]["user"]["roles"], json!(["user"]));
            assert_eq!(
                router
                    .clone()
                    .oneshot(completion(&first, &code))
                    .await?
                    .status(),
                StatusCode::UNAUTHORIZED
            );
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn existing_identity_allows_unverified_email() -> anyhow::Result<()> {
            Fixture::run(|fixture| async move {
                let router = &fixture.router;
                let signing_key = &fixture.signing_key;
                let first = start(router).await?;
                let code = signed_code(&first, "subject", true, &fixture.signing_key)?;
                assert_eq!(
                    router
                        .clone()
                        .oneshot(completion(&first, &code))
                        .await?
                        .status(),
                    StatusCode::OK
                );
                // 기대 결과: existing identity allows unverified email
                let repeated = start(router).await?;
                let unverified_code = signed_code(&repeated, "subject", false, signing_key)?;
                assert_eq!(
                    router
                        .clone()
                        .oneshot(completion(&repeated, &unverified_code))
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
            Fixture::run(|fixture| async move {
                let router = &fixture.router;
                let signing_key = &fixture.signing_key;
                // 기대 결과: login state is consumed once concurrently
                let once = start(router).await?;
                let once_code = signed_code(&once, "subject", true, signing_key)?;
                let (a, b) = tokio::join!(
                    router.clone().oneshot(completion(&once, &once_code)),
                    router.clone().oneshot(completion(&once, &once_code))
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
            Fixture::run(|fixture| async move {
                let router = &fixture.router;
                let signing_key = &fixture.signing_key;
                // 기대 결과: new identity requires verified email
                let unverified = start(router).await?;
                let unverified_code = signed_code(&unverified, "new-subject", false, signing_key)?;
                assert_eq!(
                    router
                        .clone()
                        .oneshot(completion(&unverified, &unverified_code))
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
            Fixture::run(|fixture| async move {
                let router = &fixture.router;
                let container = &fixture.container;
                let signing_key = &fixture.signing_key;
                let first = start(router).await?;
                let code = signed_code(&first, "subject", true, &fixture.signing_key)?;
                let response = router.clone().oneshot(completion(&first, &code)).await?;
                let body: Value =
                    serde_json::from_slice(&to_bytes(response.into_body(), 16384).await?)?;
                let google_id = body["data"]["claims"]["id"].as_i64().unwrap();
                // 기대 결과: inactive and withdrawn users cannot log in
                for status in ["inactive", "withdrawn"] {
                    let mut conn = container.db.conn().await?;
                    sqlx::query("UPDATE conbira_auth_users SET status = $1 WHERE id = $2")
                        .bind(status)
                        .bind(google_id)
                        .execute(conn.handle().conn())
                        .await?;
                    drop(conn);
                    let data = start(router).await?;
                    let code = signed_code(&data, "subject", true, signing_key)?;
                    assert_eq!(
                        router
                            .clone()
                            .oneshot(completion(&data, &code))
                            .await?
                            .status(),
                        StatusCode::UNAUTHORIZED
                    );
                }
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn expired_login_state_is_rejected() -> anyhow::Result<()> {
            Fixture::run(|fixture| async move {
                let router = &fixture.router;
                let container = &fixture.container;
                // 기대 결과: expired login state is rejected
                let expired = start(router).await?;
                let key = state_key(
                    expired["state"].as_str().unwrap(),
                    expired["browser_proof"].as_str().unwrap(),
                );
                let value: String = container.cache.take(&key).await?.unwrap();
                container
                    .cache
                    .set_with_ttl(&key, value, Duration::from_millis(1))
                    .await?;
                tokio::time::sleep(Duration::from_millis(10)).await;
                assert_eq!(
                    router
                        .clone()
                        .oneshot(completion(&expired, "code"))
                        .await?
                        .status(),
                    StatusCode::UNAUTHORIZED
                );
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn missing_required_inputs_are_rejected() -> anyhow::Result<()> {
            Fixture::run(|fixture| async move {
                let router = &fixture.router;
                // 기대 결과: 필수 입력이 없거나 비어 있으면 요청을 거절한다
                for body in ["{}", r#"{"code":"","state":"s","browser_proof":"p"}"#] {
                    let request = Request::post("/api/auth/login/google/complete")
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(body))?;
                    assert_eq!(
                        router.clone().oneshot(request).await?.status(),
                        StatusCode::BAD_REQUEST
                    );
                }
                Ok(())
            })
            .await
        }
    }
}
