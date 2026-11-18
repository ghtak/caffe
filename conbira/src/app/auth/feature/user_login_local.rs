pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::ApiResult;

    #[derive(serde::Deserialize)]
    pub struct LoginRequest {
        pub email: String,
        pub password: String,
    }

    impl espr::validate::Validate for LoginRequest {
        type Error = crate::Error;

        fn validate(&self) -> crate::Result<()> {
            if self.email.trim().is_empty() || self.password.is_empty() {
                return Err(crate::Error::new(
                    crate::Code::InvalidRequest,
                    "Email and password are required",
                )
                .at(crate::here!()));
            }
            Ok(())
        }
    }

    #[derive(serde::Serialize)]
    pub struct LoginResponse {
        pub access_token: String,
        pub refresh_token: String,
        pub user: crate::app::auth::entity::UserInfo,
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiJson(request): ApiJson<LoginRequest>,
    ) -> ApiResult<LoginResponse> {
        let tokens = super::usecase::execute(
            &container.db,
            &container.password_hash_service,
            &container.dummy_password_hash,
            &container.jwt_token_service,
            &container.refresh_token_service,
            super::usecase::Command {
                email: request.email,
                password: request.password,
            },
        )
        .await?;
        Ok(crate::app::http::response::ApiResponse::new(
            LoginResponse {
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
                user: tokens.user,
            },
        ))
    }
}

pub mod usecase {
    use crate::app::DatabaseExt;
    use crate::app::auth::claims::UserClaims;
    use crate::app::auth::entity::UserStatus;
    use crate::app::auth::repository::{identity, user};
    use crate::app::auth::service::password_hash::PasswordHashService;
    use crate::app::auth::service::refresh_token::RefreshTokenService;
    use crate::app::container::db::Database;
    use crate::error::ResultExt as _;

    pub struct Command {
        pub email: String,
        pub password: String,
    }

    pub struct Output {
        pub access_token: String,
        pub refresh_token: String,
        pub user: crate::app::auth::entity::UserInfo,
    }

    pub async fn execute(
        db: &Database,
        password_hasher: &PasswordHashService,
        dummy_password_hash: &str,
        jwt_token_service: &espr::jwt::TokenService,
        refresh_token_service: &RefreshTokenService,
        command: Command,
    ) -> crate::Result<Output> {
        let unauthorized = || {
            crate::Error::new(
                crate::Code::Unauthorized,
                "Invalid credentials or user status",
            )
        };
        let email = command.email.trim().to_ascii_lowercase();

        let identity = {
            let mut connection = db.acquire_conn(crate::here!()).await?;
            identity::find_local_by_email(&mut connection.handle(), &email).await?
        };

        let hash = identity
            .as_ref()
            .and_then(|identity| identity.password_hash.as_deref())
            .unwrap_or(dummy_password_hash)
            .to_owned();

        let valid = password_hasher.verify(command.password, hash).await?;
        if !valid
            || identity
                .as_ref()
                .is_none_or(|identity| identity.password_hash.is_none())
        {
            return Err(unauthorized().at(crate::here!()));
        }
        let identity = identity.ok_or_else(|| unauthorized().at(crate::here!()))?;
        let user = {
            let mut connection = db.acquire_conn(crate::here!()).await?;
            user::find_by_id(&mut connection.handle(), identity.user_id)
                .await?
                .ok_or_else(|| {
                    crate::Error::internal("User for identity not found").at(crate::here!())
                })?
        };
        if !matches!(user.status, UserStatus::Registered | UserStatus::Active) {
            return Err(unauthorized().at(crate::here!()));
        }

        let claims = UserClaims::new(user.id, user.status).try_into()?;
        let access = jwt_token_service
            .issue(&user.id.to_string(), claims)
            .map_err_internal("Failed to issue access token", crate::here!())?;

        let refresh = {
            let mut tx = db.begin_tx(crate::here!()).await?;
            let refresh = refresh_token_service
                .issue(&mut tx.handle(), user.id)
                .await?;
            tx.commit()
                .await
                .map_err_internal("Failed to commit login", crate::here!())?;
            refresh
        };

        Ok(Output {
            access_token: access.token,
            refresh_token: refresh.token,
            user: crate::app::auth::entity::UserInfo {
                id: user.id,
                nickname: user.nickname,
                status: user.status,
                email: identity.email,
                created_at: user.created_at,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use espr::hash::PasswordHasher as _;

    use super::usecase::{Command, execute};
    use crate::app::auth::feature::user_create_local;
    use crate::app::auth::{self};
    use crate::app::container::db::Database;

    #[tokio::test]
    async fn login_issues_tokens_only_for_valid_credentials_and_allowed_status()
    -> anyhow::Result<()> {
        let db = Database::new(&crate::test_support::database_config(1)).await?;
        let mut connection = db.conn().await?;
        for schema in [
            include_str!("../../../../migrations/0010_user.sql"),
            include_str!("../../../../migrations/0011_refresh_token.sql"),
        ] {
            let schema = schema.replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE");
            sqlx::raw_sql(sqlx::AssertSqlSafe(schema))
                .execute(connection.handle().conn())
                .await?;
        }
        drop(connection);
        let dir = std::env::temp_dir().join(format!("conbira-login-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir)?;
        let keys = espr::jwt::ed25519::Config {
            key_path: dir.join("private.pem"),
            public_key_path: dir.join("public.pem"),
        };
        espr::jwt::ed25519::generate(&keys)?;
        let jwt = espr::jwt::TokenService::new(&espr::jwt::Config {
            issuer: "test".into(),
            audience: "test".into(),
            access_token_ttl_seconds: 900,
            ed25519: keys,
        })?;
        std::fs::remove_dir_all(dir)?;
        let refresh = auth::service::refresh_token::RefreshTokenService::new(
            auth::service::refresh_token::Config {
                ttl_seconds: 3600,
                family_ttl_seconds: 7200,
            },
        );
        let hasher = Arc::new(espr::hash::argon2::Hasher::new(&Default::default())?);
        let password_service = crate::app::auth::service::password_hash::PasswordHashService::new(
            Arc::clone(&hasher),
            &crate::app::auth::service::password_hash::Config {
                max_concurrency: 4,
                wait_timeout_seconds: 5,
            },
        )?;
        let dummy_hash = hasher.hash("dummy login password")?;
        let user = user_create_local::usecase::execute(
            &db,
            &password_service,
            user_create_local::usecase::Command {
                nickname: "Alice".into(),
                email: "alice@example.com".into(),
                password: "secret".into(),
            },
        )
        .await?;
        for status in ["registered", "active", "inactive", "withdrawn"] {
            let mut connection = db.conn().await?;
            sqlx::query("UPDATE conbira_users SET status = $1 WHERE id = $2")
                .bind(status)
                .bind(user.id)
                .execute(connection.handle().conn())
                .await?;
            drop(connection);
            let result = execute(
                &db,
                &password_service,
                &dummy_hash,
                &jwt,
                &refresh,
                Command {
                    email: " Alice@Example.com ".into(),
                    password: "secret".into(),
                },
            )
            .await;
            if status == "registered" || status == "active" {
                let tokens = result?;
                // 허용된 계정의 JWT와 저장된 refresh token이 같은 사용자를
                // 가리킨다.
                let claims = jwt.verify(&tokens.access_token)?;
                assert_eq!(claims.sub, user.id.to_string());
                assert_eq!(
                    claims.user,
                    serde_json::json!({"id": user.id, "status": status})
                        .as_object()
                        .unwrap()
                        .clone()
                );
                assert_eq!(tokens.user.id, user.id);
                assert_eq!(tokens.user.email, "alice@example.com");
                assert_eq!(tokens.user.nickname, "Alice");
                let mut connection = db.conn().await?;
                assert_eq!(
                    refresh
                        .find_user_id(&mut connection.handle(), &tokens.refresh_token)
                        .await?,
                    Some(user.id)
                );
            } else {
                assert!(matches!(result, Err(error) if error.code == crate::Code::Unauthorized));
            }
        }
        for (email, password) in [
            ("missing@example.com", "secret"),
            ("missing@example.com", "dummy login password"),
            ("alice@example.com", "wrong"),
        ] {
            let result = execute(
                &db,
                &password_service,
                &dummy_hash,
                &jwt,
                &refresh,
                Command {
                    email: email.into(),
                    password: password.into(),
                },
            )
            .await;
            assert!(matches!(result, Err(error) if error.code == crate::Code::Unauthorized));
        }
        // 더미 해시가 잘못되면 없는 계정도 검증 오류를 반환한다.
        let result = execute(
            &db,
            &password_service,
            "invalid hash",
            &jwt,
            &refresh,
            Command {
                email: "missing@example.com".into(),
                password: "secret".into(),
            },
        )
        .await;
        assert!(matches!(result, Err(error) if error.code == crate::Code::Internal));
        let mut connection = db.conn().await?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM conbira_refresh_tokens")
            .fetch_one(connection.handle().conn())
            .await?;
        // 실패한 시도는 refresh token을 추가하지 않는다.
        assert_eq!(count, 2);
        Ok(())
    }
}
