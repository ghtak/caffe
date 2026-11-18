use crate::Result;
use crate::app::auth::entity::{Identity, Provider};
use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

pub struct NewIdentity {
    pub user_id: i64,
    pub provider: Provider,
    pub provider_subject: String,
    pub email: String,
    pub password_hash: Option<String>,
}

pub async fn insert(handle: &mut Handle<'_>, identity: &NewIdentity) -> Result<Identity> {
    sqlx::query_as::<_, Identity>(
        "INSERT INTO conbira_user_identities
             (user_id, provider, provider_subject, email, password_hash)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, user_id, provider, provider_subject, email, password_hash,
                   created_at, updated_at",
    )
    .bind(identity.user_id)
    .bind(identity.provider)
    .bind(&identity.provider_subject)
    .bind(&identity.email)
    .bind(&identity.password_hash)
    .fetch_one(handle.conn())
    .await
    .internal(
        "Failed to insert identity",
        "auth.repository.identity.insert",
    )
}

pub async fn find_by_provider_subject(
    handle: &mut Handle<'_>,
    provider: Provider,
    subject: &str,
) -> Result<Option<Identity>> {
    sqlx::query_as::<_, Identity>(
        "SELECT id, user_id, provider, provider_subject, email, password_hash,
                created_at, updated_at
         FROM conbira_user_identities
         WHERE provider = $1 AND provider_subject = $2",
    )
    .bind(provider)
    .bind(subject)
    .fetch_optional(handle.conn())
    .await
    .internal(
        "Failed to find identity by provider and subject",
        "auth.repository.identity.find_by_provider_subject",
    )
}

pub async fn find_local_by_email(handle: &mut Handle<'_>, email: &str) -> Result<Option<Identity>> {
    sqlx::query_as::<_, Identity>(
        "SELECT id, user_id, provider, provider_subject, email, password_hash,
                created_at, updated_at
         FROM conbira_user_identities
         WHERE provider = 'local' AND email = $1",
    )
    .bind(email)
    .fetch_optional(handle.conn())
    .await
    .internal(
        "Failed to find local identity by email",
        "auth.repository.identity.find_local_by_email",
    )
}

pub async fn find_by_user_id(handle: &mut Handle<'_>, user_id: i64) -> Result<Vec<Identity>> {
    sqlx::query_as::<_, Identity>(
        "SELECT id, user_id, provider, provider_subject, email, password_hash,
                created_at, updated_at
         FROM conbira_user_identities
         WHERE user_id = $1
         ORDER BY id",
    )
    .bind(user_id)
    .fetch_all(handle.conn())
    .await
    .internal(
        "Failed to find identities by user",
        "auth.repository.identity.find_by_user_id",
    )
}

pub async fn update(handle: &mut Handle<'_>, identity: &Identity) -> Result<Option<Identity>> {
    sqlx::query_as::<_, Identity>(
        "UPDATE conbira_user_identities
         SET email = $2, password_hash = $3, updated_at = CURRENT_TIMESTAMP
         WHERE id = $1
         RETURNING id, user_id, provider, provider_subject, email, password_hash,
                   created_at, updated_at",
    )
    .bind(identity.id)
    .bind(&identity.email)
    .bind(&identity.password_hash)
    .fetch_optional(handle.conn())
    .await
    .internal(
        "Failed to update identity",
        "auth.repository.identity.update",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::container::db::Database;

    #[tokio::test]
    async fn stores_finds_and_updates_local_identity() -> anyhow::Result<()> {
        let database = Database::new(&crate::test_support::database_config(1)).await?;
        let mut connection = database.conn().await?;
        let mut handle = connection.handle();
        let schema = include_str!("../../../../migrations/0010_user.sql")
            .replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE");
        sqlx::raw_sql(sqlx::AssertSqlSafe(schema))
            .execute(handle.conn())
            .await?;
        let user_id: i64 =
            sqlx::query_scalar("INSERT INTO conbira_users (nickname) VALUES ('user') RETURNING id")
                .fetch_one(handle.conn())
                .await?;
        let input = NewIdentity {
            user_id,
            provider: Provider::Local,
            provider_subject: user_id.to_string(),
            email: "user@example.com".into(),
            password_hash: Some("hash".into()),
        };
        let mut identity = insert(&mut handle, &input).await?;
        // 직접 로그인 식별 정보는 공급자·이메일·사용자 각각으로 조회된다.
        assert_eq!(identity.provider, Provider::Local);
        assert_eq!(identity.created_at, identity.updated_at);
        assert!(
            find_by_provider_subject(&mut handle, Provider::Local, &input.provider_subject)
                .await?
                .as_ref()
                == Some(&identity)
        );
        assert!(
            find_local_by_email(&mut handle, &input.email)
                .await?
                .as_ref()
                == Some(&identity)
        );
        let identities = find_by_user_id(&mut handle, user_id).await?;
        assert!(identities.as_slice() == std::slice::from_ref(&identity));
        assert!(
            find_local_by_email(&mut handle, "missing@example.com")
                .await?
                .is_none()
        );
        assert!(
            find_by_provider_subject(&mut handle, Provider::Local, "missing")
                .await?
                .is_none()
        );
        assert!(find_by_user_id(&mut handle, -1).await?.is_empty());

        let second_user_id: i64 = sqlx::query_scalar(
            "INSERT INTO conbira_users (nickname) VALUES ('second') RETURNING id",
        )
        .fetch_one(handle.conn())
        .await?;
        let duplicate = NewIdentity {
            user_id: second_user_id,
            provider: Provider::Local,
            provider_subject: second_user_id.to_string(),
            email: input.email.clone(),
            password_hash: Some("hash".into()),
        };
        // 서로 다른 사용자의 직접 로그인 이메일은 중복 저장할 수 없다.
        assert!(insert(&mut handle, &duplicate).await.is_err());

        let old_timestamp = jiff::Timestamp::from_second(1_700_000_000)?;
        sqlx::query("UPDATE conbira_user_identities SET updated_at = $2 WHERE id = $1")
            .bind(identity.id)
            .bind(jiff_sqlx::Timestamp::from(old_timestamp))
            .execute(handle.conn())
            .await?;
        identity.email = "new@example.com".into();
        identity.password_hash = Some("new hash".into());
        let updated = update(&mut handle, &identity).await?.unwrap();
        // 변경된 이메일과 해시가 저장되고 수정 시각이 갱신된다.
        assert_eq!(updated.email, "new@example.com");
        assert_eq!(updated.password_hash.as_deref(), Some("new hash"));
        assert_eq!(updated.created_at, identity.created_at);
        assert!(updated.updated_at > old_timestamp);
        assert!(
            find_local_by_email(&mut handle, &input.email)
                .await?
                .is_none()
        );
        assert!(
            find_local_by_email(&mut handle, "new@example.com")
                .await?
                .as_ref()
                == Some(&updated)
        );
        identity.id = -1;
        assert!(update(&mut handle, &identity).await?.is_none());
        Ok(())
    }
}
