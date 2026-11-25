pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::ApiResult;

    #[derive(serde::Deserialize)]
    pub struct RequestBody {
        pub email: String,
        pub password: String,
    }

    impl espr::validate::Validate for RequestBody {
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

    pub type ResponseData = crate::app::auth::service::login_tokens::LoginTokens;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiJson(request): ApiJson<RequestBody>,
    ) -> ApiResult<ResponseData> {
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
        Ok(tokens.into())
    }
}

pub mod usecase {
    use crate::app::DatabaseExt;
    use crate::app::auth::repository::identity;
    use crate::app::auth::service::login_tokens::{self, LoginTokens};
    use crate::app::auth::service::password_hash::PasswordHashService;
    use crate::app::auth::service::refresh_token::RefreshTokenService;
    use crate::app::container::db::Database;

    pub struct Command {
        pub email: String,
        pub password: String,
    }

    pub async fn execute(
        db: &Database,
        password_hasher: &PasswordHashService,
        dummy_password_hash: &str,
        jwt_token_service: &espr::jwt::TokenService,
        refresh_token_service: &RefreshTokenService,
        command: Command,
    ) -> crate::Result<LoginTokens> {
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
        login_tokens::issue(
            db,
            jwt_token_service,
            refresh_token_service,
            identity.user_id,
            identity.email,
        )
        .await
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

    struct LoginFixture {
        db: Database,
        password_service: auth::service::password_hash::PasswordHashService,
        dummy_hash: String,
        jwt: espr::jwt::TokenService,
        refresh: auth::service::refresh_token::RefreshTokenService,
    }

    impl LoginFixture {
        async fn new() -> anyhow::Result<Self> {
            let db = Database::new(&crate::test_support::database_config(1)).await?;
            let mut connection = db.conn().await?;
            for schema in [
                include_str!("../../../../migrations/0010_user.sql"),
                include_str!("../../../../migrations/0011_refresh_token.sql"),
                include_str!("../../../../migrations/0012_rbac.sql"),
            ] {
                let schema = schema.replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE");
                sqlx::raw_sql(sqlx::AssertSqlSafe(schema))
                    .execute(connection.handle().conn())
                    .await?;
            }
            drop(connection);
            let jwt = crate::test_support::jwt_service("test", "test", 900)?;
            let refresh = auth::service::refresh_token::RefreshTokenService::new(
                auth::service::refresh_token::Config {
                    ttl_seconds: 3600,
                    family_ttl_seconds: 7200,
                },
            );
            let hasher = Arc::new(espr::hash::argon2::Hasher::new(&Default::default())?);
            let password_service =
                crate::app::auth::service::password_hash::PasswordHashService::new(
                    Arc::clone(&hasher),
                    &crate::app::auth::service::password_hash::Config {
                        max_concurrency: 4,
                        wait_timeout_seconds: 5,
                    },
                )?;
            let dummy_hash = hasher.hash("dummy login password")?;
            Ok(Self {
                db,
                password_service,
                dummy_hash,
                jwt,
                refresh,
            })
        }

        async fn create_user(
            &self,
            status: auth::entity::UserStatus,
        ) -> anyhow::Result<auth::entity::UserInfo> {
            Ok(user_create_local::usecase::execute(
                &self.db,
                &self.password_service,
                user_create_local::usecase::Command {
                    nickname: "Alice".into(),
                    email: "alice@example.com".into(),
                    password: "secret".into(),
                    role_codes: vec!["user".into()],
                    status,
                },
            )
            .await?)
        }

        async fn refresh_token_count(&self) -> anyhow::Result<i64> {
            let mut connection = self.db.conn().await?;
            Ok(
                sqlx::query_scalar("SELECT count(*) FROM conbira_auth_refresh_tokens")
                    .fetch_one(connection.handle().conn())
                    .await?,
            )
        }
    }

    mod contract {
        use auth::entity::UserStatus;

        use super::*;

        #[tokio::test]
        async fn allowed_accounts_receive_tokens_with_roles_at_issue_time() -> anyhow::Result<()> {
            for (status, status_claim) in [
                (UserStatus::Registered, "registered"),
                (UserStatus::Active, "active"),
            ] {
                let fixture = LoginFixture::new().await?;
                let user = fixture.create_user(status).await?;
                let tokens = execute(
                    &fixture.db,
                    &fixture.password_service,
                    &fixture.dummy_hash,
                    &fixture.jwt,
                    &fixture.refresh,
                    Command {
                        email: " Alice@Example.com ".into(),
                        password: "secret".into(),
                    },
                )
                .await?;

                let mut connection = fixture.db.conn().await?;
                sqlx::query("DELETE FROM conbira_rbac_user_roles WHERE user_id = $1")
                    .bind(user.id)
                    .execute(connection.handle().conn())
                    .await?;
                drop(connection);

                let claims = fixture.jwt.verify(&tokens.access_token)?;
                // 발급된 JWT는 역할을 회수한 뒤에도 발급 시점의 역할과 권한을
                // 담고, 사용자 응답은 같은 사용자를 가리킨다.
                assert_eq!(claims.sub, user.id.to_string());
                assert_eq!(
                    serde_json::Value::Object(claims.user),
                    serde_json::json!({"id": user.id, "status": status_claim, "roles": ["user"], "permissions": ["rbac.read"]})
                );
                assert_eq!(tokens.user.id, user.id);
                assert_eq!(tokens.user.email, "alice@example.com");
                assert_eq!(tokens.user.nickname, "Alice");
                let new_tokens = execute(
                    &fixture.db,
                    &fixture.password_service,
                    &fixture.dummy_hash,
                    &fixture.jwt,
                    &fixture.refresh,
                    Command {
                        email: "alice@example.com".into(),
                        password: "secret".into(),
                    },
                )
                .await?;
                let new_claims = fixture.jwt.verify(&new_tokens.access_token)?;
                // 역할을 회수한 후 발급한 토큰에는 역할과 권한이 없다.
                assert_eq!(new_claims.user["roles"], serde_json::json!([]));
                assert_eq!(new_claims.user["permissions"], serde_json::json!([]));

                let mut connection = fixture.db.conn().await?;
                let stored_user_id = fixture
                    .refresh
                    .find_user_id(&mut connection.handle(), &tokens.refresh_token)
                    .await?;
                drop(connection);
                // 재로그인 후에도 첫 리프레시 토큰은 같은 사용자를 가리킨다.
                assert_eq!(stored_user_id, Some(user.id));
                assert_eq!(fixture.refresh_token_count().await?, 2);
            }
            Ok(())
        }

        #[tokio::test]
        async fn inactive_or_withdrawn_accounts_cannot_log_in() -> anyhow::Result<()> {
            for status in [UserStatus::Inactive, UserStatus::Withdrawn] {
                let fixture = LoginFixture::new().await?;
                fixture.create_user(status).await?;
                let result = execute(
                    &fixture.db,
                    &fixture.password_service,
                    &fixture.dummy_hash,
                    &fixture.jwt,
                    &fixture.refresh,
                    Command {
                        email: " Alice@Example.com ".into(),
                        password: "secret".into(),
                    },
                )
                .await;
                // 비활성·탈퇴 계정은 401에 해당하는 오류를 반환하고 토큰을
                // 저장하지 않는다.
                assert!(matches!(result, Err(error) if error.code == crate::Code::Unauthorized));
                assert_eq!(fixture.refresh_token_count().await?, 0);
            }
            Ok(())
        }

        #[tokio::test]
        async fn invalid_credentials_do_not_issue_tokens() -> anyhow::Result<()> {
            let fixture = LoginFixture::new().await?;
            fixture.create_user(UserStatus::Active).await?;
            for (email, password) in [
                ("missing@example.com", "secret"),
                ("missing@example.com", "dummy login password"),
                ("alice@example.com", "wrong"),
            ] {
                let result = execute(
                    &fixture.db,
                    &fixture.password_service,
                    &fixture.dummy_hash,
                    &fixture.jwt,
                    &fixture.refresh,
                    Command {
                        email: email.into(),
                        password: password.into(),
                    },
                )
                .await;
                // 등록되지 않은 이메일과 잘못된 비밀번호에 같은 인증 오류를
                // 반환한다.
                assert!(
                    matches!(result, Err(error) if error.code == crate::Code::Unauthorized),
                    "{email}"
                );
            }
            // 실패한 로그인은 리프레시 토큰을 저장하지 않는다.
            assert_eq!(fixture.refresh_token_count().await?, 0);
            Ok(())
        }
    }

    mod edge {
        use super::*;

        #[tokio::test]
        async fn malformed_dummy_hash_returns_an_internal_error() -> anyhow::Result<()> {
            let fixture = LoginFixture::new().await?;
            let result = execute(
                &fixture.db,
                &fixture.password_service,
                "invalid hash",
                &fixture.jwt,
                &fixture.refresh,
                Command {
                    email: "missing@example.com".into(),
                    password: "secret".into(),
                },
            )
            .await;
            // 존재하지 않는 계정의 검증에 사용하는 해시가 잘못되면 내부 오류를
            // 반환한다.
            assert!(matches!(result, Err(error) if error.code == crate::Code::Internal));
            assert_eq!(fixture.refresh_token_count().await?, 0);
            Ok(())
        }
    }
}
