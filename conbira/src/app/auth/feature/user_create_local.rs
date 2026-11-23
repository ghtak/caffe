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
            role_codes: Vec::new(),
            status: crate::app::auth::entity::UserStatus::Registered,
        }
    }
}

pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::auth::entity::UserInfo;
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::ApiResult;

    pub type RequestBody = super::Input;

    pub type ResponseData = UserInfo;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiJson(request): ApiJson<RequestBody>,
    ) -> ApiResult<ResponseData> {
        let mut command: super::usecase::Command = request.into();
        command.role_codes = vec!["user".to_owned()];
        let user =
            super::usecase::execute(&container.db, &container.password_hash_service, command)
                .await?;
        Ok(user.into())
    }
}

pub mod usecase {
    use crate::app::DatabaseExt as _;
    use crate::app::auth::entity::{Provider, UserInfo};
    use crate::app::auth::repository::{identity, user};
    use crate::app::auth::service::password_hash::PasswordHashService;
    use crate::app::container::db::Database;
    use crate::app::rbac::repository::{role, user_role};
    use crate::error::ResultExt as _;

    pub struct Command {
        pub nickname: String,
        pub email: String,
        pub password: String,
        pub role_codes: Vec<String>,
        pub status: crate::app::auth::entity::UserStatus,
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
        let user = user::insert(
            &mut handle,
            &user::NewUser {
                nickname,
                status: command.status,
            },
        )
        .await?;
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
        assign_roles(&mut handle, user.id, &command.role_codes).await?;
        tx.commit()
            .await
            .map_err_internal("Failed to commit user creation", crate::here!())?;

        Ok(UserInfo {
            id: user.id,
            nickname: user.nickname,
            status: user.status,
            email,
            created_at: user.created_at,
        })
    }

    async fn assign_roles(
        handle: &mut crate::app::db::Handle<'_>,
        user_id: i64,
        role_codes: &[String],
    ) -> crate::Result<()> {
        let mut seen = std::collections::HashSet::new();
        for code in role_codes {
            if !seen.insert(code) {
                continue;
            }
            let role = role::find_by_code(handle, code).await?.ok_or_else(|| {
                crate::Error::new(crate::Code::NotFound, format!("Role not found: {code}"))
                    .at(crate::here!())
            })?;
            user_role::insert(handle, user_id, role.id).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    mod contract {
        use std::sync::Arc;

        use espr::hash::PasswordHasher as _;
        use espr::validate::Validate as _;

        use super::super::Input;
        use super::super::endpoint::RequestBody;
        use super::super::usecase::{Command, execute};
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
        fn local_user_request_defaults_to_registered_status() {
            let request: RequestBody = serde_json::from_str(
                r#"{"nickname":"user","email":"user@example.com","password":"secret"}"#,
            )
            .unwrap();
            let command: Command = request.into();
            // 로컬 사용자 생성은 등록 상태로 시작한다.
            assert_eq!(command.status, UserStatus::Registered);
        }

        async fn setup() -> anyhow::Result<(
            Database,
            Arc<espr::hash::argon2::Hasher>,
            crate::app::auth::service::password_hash::PasswordHashService,
        )> {
            let db = Database::new(&crate::test_support::database_config(1)).await?;
            let mut connection = db.conn().await?;
            let schema = include_str!("../../../../migrations/0010_user.sql")
                .replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE");
            sqlx::raw_sql(sqlx::AssertSqlSafe(schema))
                .execute(connection.handle().conn())
                .await?;
            let schema = include_str!("../../../../migrations/0012_rbac.sql")
                .replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE");
            sqlx::raw_sql(sqlx::AssertSqlSafe(schema))
                .execute(connection.handle().conn())
                .await?;
            drop(connection);

            let hasher = Arc::new(espr::hash::argon2::Hasher::new(&Default::default())?);
            let password_service =
                crate::app::auth::service::password_hash::PasswordHashService::new(
                    Arc::clone(&hasher),
                    &crate::app::auth::service::password_hash::Config {
                        max_concurrency: 4,
                        wait_timeout_seconds: 5,
                    },
                )?;
            Ok((db, hasher, password_service))
        }

        #[tokio::test]
        async fn creates_local_user_with_hash_and_distinct_roles() -> anyhow::Result<()> {
            let (db, hasher, password_service) = setup().await?;
            let result = execute(
                &db,
                &password_service,
                Command {
                    nickname: "Alice".into(),
                    email: " Alice@Example.com ".into(),
                    password: "correct password".into(),
                    role_codes: vec!["user".into(), "admin".into(), "user".into()],
                    status: UserStatus::Active,
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
            assert_eq!(result.status, UserStatus::Active);
            assert_eq!(
                crate::app::auth::repository::user::find_by_id(&mut handle, result.id)
                    .await?
                    .unwrap()
                    .status,
                UserStatus::Active
            );
            assert_eq!(identity.user_id, result.id);
            assert_eq!(identity.provider, Provider::Local);
            assert_eq!(identity.provider_subject, result.id.to_string());
            assert!(hasher.verify(
                "correct password",
                identity.password_hash.as_deref().unwrap()
            )?);
            assert!(!hasher.verify("wrong password", identity.password_hash.as_deref().unwrap())?);
            let roles =
                crate::app::rbac::repository::user_role::list_by_user_id(&mut handle, result.id)
                    .await?;
            assert_eq!(roles.len(), 2);
            assert!(roles.iter().any(|role| role.code == "user"));
            assert!(roles.iter().any(|role| role.code == "admin"));
            Ok(())
        }

        #[tokio::test]
        async fn duplicate_email_rolls_back_user_creation() -> anyhow::Result<()> {
            let (db, _, password_service) = setup().await?;
            execute(
                &db,
                &password_service,
                Command {
                    nickname: "Original".into(),
                    email: "alice@example.com".into(),
                    password: "password".into(),
                    role_codes: vec!["user".into()],
                    status: UserStatus::Registered,
                },
            )
            .await?;
            assert!(
                execute(
                    &db,
                    &password_service,
                    Command {
                        nickname: "Duplicate".into(),
                        email: "alice@example.com".into(),
                        password: "another password".into(),
                        role_codes: vec!["user".into()],
                        status: UserStatus::Registered,
                    }
                )
                .await
                .is_err()
            );
            let mut connection = db.conn().await?;
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM conbira_auth_users")
                .fetch_one(connection.handle().conn())
                .await?;
            // 중복 이메일 요청은 추가 사용자를 남기지 않는다.
            assert_eq!(count, 1);
            Ok(())
        }

        #[tokio::test]
        async fn missing_role_rolls_back_user_creation() -> anyhow::Result<()> {
            let (db, _, password_service) = setup().await?;
            let error = execute(
                &db,
                &password_service,
                Command {
                    nickname: "Missing role".into(),
                    email: "missing@example.com".into(),
                    password: "password".into(),
                    role_codes: vec!["user".into(), "missing".into()],
                    status: UserStatus::Registered,
                },
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, crate::Code::NotFound);
            let mut connection = db.conn().await?;
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM conbira_auth_users")
                .fetch_one(connection.handle().conn())
                .await?;
            // 없는 역할이 포함되면 생성한 사용자도 롤백한다.
            assert_eq!(count, 0);
            Ok(())
        }
    }
}
