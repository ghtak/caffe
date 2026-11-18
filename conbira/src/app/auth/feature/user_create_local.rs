// CLI에서도 사용하므로 예외적으로 endpoint 모듈 밖에 둔다.
#[derive(serde::Deserialize)]
pub struct Input {
    pub nickname: String,
    pub email: String,
    pub password: String,
}

impl espr::validate::Validate for Input {
    type Error = crate::Error;

    fn validate(&self) -> crate::Result<()> {
        if self.nickname.trim().is_empty()
            || self.email.trim().is_empty()
            || self.password.is_empty()
        {
            return Err(crate::Error::new(
                crate::Code::InvalidRequest,
                "Nickname, email and password are required",
            )
            .at(crate::here!()));
        }
        Ok(())
    }
}

impl From<Input> for usecase::Command {
    fn from(request: Input) -> Self {
        Self {
            nickname: request.nickname,
            email: request.email,
            password: request.password,
        }
    }
}

pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::auth::entity::UserInfo;
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::{ApiResponse, ApiResult};

    pub type CreateRequest = super::Input;

    pub type CreateResponse = UserInfo;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiJson(request): ApiJson<CreateRequest>,
    ) -> ApiResult<CreateResponse> {
        let user = super::usecase::execute(
            &container.db,
            &container.password_hash_service,
            request.into(),
        )
        .await?;
        Ok(ApiResponse::new(user))
    }
}

pub mod usecase {
    use crate::app::DatabaseExt as _;
    use crate::app::auth::entity::{Provider, UserInfo};
    use crate::app::auth::repository::{identity, user};
    use crate::app::auth::service::password_hash::PasswordHashService;
    use crate::app::container::db::Database;
    use crate::error::ResultExt as _;

    pub struct Command {
        pub nickname: String,
        pub email: String,
        pub password: String,
    }

    pub async fn execute(
        db: &Database,
        password_hasher: &PasswordHashService,
        command: Command,
    ) -> crate::Result<UserInfo> {
        let nickname = command.nickname.trim().to_owned();
        let email = command.email.trim().to_ascii_lowercase();
        let password_hash = password_hasher.hash(command.password).await?;

        let mut tx = db.begin_tx(crate::here!()).await?;
        let mut handle = tx.handle();
        let user = user::insert(&mut handle, &user::NewUser { nickname }).await?;
        identity::insert(
            &mut handle,
            &identity::NewIdentity {
                user_id: user.id,
                provider: Provider::Local,
                provider_subject: user.id.to_string(),
                email: email.clone(),
                password_hash: Some(password_hash),
            },
        )
        .await?;
        tx.commit()
            .await
            .internal("Failed to commit user creation", crate::here!())?;

        Ok(UserInfo {
            id: user.id,
            nickname: user.nickname,
            status: user.status,
            email,
            created_at: user.created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use espr::hash::PasswordHasher as _;
    use espr::validate::Validate as _;

    use super::Input;
    use super::endpoint::CreateRequest;
    use super::usecase::{Command, execute};
    use crate::app::auth::entity::{Provider, UserStatus};
    use crate::app::auth::repository::identity;
    use crate::app::container::db::Database;

    #[test]
    fn rejects_empty_required_fields() {
        for (nickname, email, password) in [
            ("  ", "user@example.com", "password"),
            ("user", "  ", "password"),
            ("user", "user@example.com", ""),
        ] {
            let request = Input {
                nickname: nickname.into(),
                email: email.into(),
                password: password.into(),
            };
            // 필수 입력값이 비어 있으면 요청 오류를 반환한다.
            assert!(
                matches!(request.validate(), Err(error) if error.code == crate::Code::InvalidRequest)
            );
        }
        let request = Input {
            nickname: "user".into(),
            email: "user@example.com".into(),
            password: "password".into(),
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn request_deserializes_and_converts_to_command() {
        let request: CreateRequest = serde_json::from_str(
            r#"{"nickname":"user","email":"user@example.com","password":"secret"}"#,
        )
        .unwrap();
        assert!(request.validate().is_ok());
        let command: Command = request.into();
        // JSON 요청의 세 값이 유스케이스 입력으로 전달된다.
        assert_eq!(command.nickname, "user");
        assert_eq!(command.email, "user@example.com");
        assert_eq!(command.password, "secret");
    }

    #[tokio::test]
    async fn creates_local_user_with_hash_and_rolls_back_duplicate_email() -> anyhow::Result<()> {
        let db = Database::new(&crate::test_support::database_config(1)).await?;
        let mut connection = db.conn().await?;
        let schema = include_str!("../../../../migrations/0010_user.sql")
            .replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE");
        sqlx::raw_sql(sqlx::AssertSqlSafe(schema))
            .execute(connection.handle().conn())
            .await?;
        drop(connection);

        let hasher = Arc::new(espr::hash::argon2::Hasher::new(&Default::default())?);
        let password_service = crate::app::auth::service::password_hash::PasswordHashService::new(
            Arc::clone(&hasher),
            &crate::app::auth::service::password_hash::Config {
                max_concurrency: 4,
                wait_timeout_seconds: 5,
            },
        )?;
        let result = execute(
            &db,
            &password_service,
            Command {
                nickname: "Alice".into(),
                email: " Alice@Example.com ".into(),
                password: "correct password".into(),
            },
        )
        .await?;

        let mut connection = db.conn().await?;
        let mut handle = connection.handle();
        let identity = identity::find_local_by_email(&mut handle, "alice@example.com")
            .await?
            .unwrap();
        // 생성 결과와 식별 정보가 연결되고 비밀번호는 검증 가능한 해시로
        // 저장된다.
        assert_eq!(result.nickname, "Alice");
        assert_eq!(result.email, "alice@example.com");
        assert_eq!(result.status, UserStatus::Registered);
        assert_eq!(identity.user_id, result.id);
        assert_eq!(identity.provider, Provider::Local);
        assert_eq!(identity.provider_subject, result.id.to_string());
        assert!(hasher.verify(
            "correct password",
            identity.password_hash.as_deref().unwrap()
        )?);
        assert!(!hasher.verify("wrong password", identity.password_hash.as_deref().unwrap())?);
        drop(connection);

        assert!(
            execute(
                &db,
                &password_service,
                Command {
                    nickname: "Duplicate".into(),
                    email: "alice@example.com".into(),
                    password: "another password".into(),
                }
            )
            .await
            .is_err()
        );
        let mut connection = db.conn().await?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM conbira_users")
            .fetch_one(connection.handle().conn())
            .await?;
        // 식별 정보 중복으로 실패한 사용자는 남지 않는다.
        assert_eq!(count, 1);
        Ok(())
    }
}
