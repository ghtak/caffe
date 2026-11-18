use crate::Result;
use crate::app::auth::entity::RefreshToken;
use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

pub struct NewRefreshToken {
    pub user_id: i64,
    pub family_id: uuid::Uuid,
    pub token_hash: String,
    pub expires_at: jiff::Timestamp,
    pub family_expires_at: jiff::Timestamp,
}

#[derive(sqlx::FromRow)]
struct TokenRow {
    id: i64,
    user_id: i64,
    family_id: uuid::Uuid,
    token_hash: String,
    expires_at: jiff_sqlx::Timestamp,
    family_expires_at: jiff_sqlx::Timestamp,
    used_at: Option<jiff_sqlx::Timestamp>,
    revoked_at: Option<jiff_sqlx::Timestamp>,
    revoke_reason: Option<String>,
    created_at: jiff_sqlx::Timestamp,
}

impl From<TokenRow> for RefreshToken {
    fn from(row: TokenRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            family_id: row.family_id,
            token_hash: row.token_hash,
            expires_at: row.expires_at.into(),
            family_expires_at: row.family_expires_at.into(),
            used_at: row.used_at.map(Into::into),
            revoked_at: row.revoked_at.map(Into::into),
            revoke_reason: row.revoke_reason,
            created_at: row.created_at.into(),
        }
    }
}

pub async fn insert(handle: &mut Handle<'_>, token: &NewRefreshToken) -> Result<RefreshToken> {
    sqlx::query_as::<_, TokenRow>(
        "INSERT INTO conbira_refresh_tokens
             (user_id, family_id, token_hash, expires_at, family_expires_at)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, user_id, family_id, token_hash, expires_at, family_expires_at,
                   used_at, revoked_at, revoke_reason, created_at",
    )
    .bind(token.user_id)
    .bind(token.family_id)
    .bind(&token.token_hash)
    .bind(jiff_sqlx::Timestamp::from(token.expires_at))
    .bind(jiff_sqlx::Timestamp::from(token.family_expires_at))
    .fetch_one(handle.conn())
    .await
    .map(Into::into)
    .internal(
        "Failed to insert refresh token",
        "auth.repository.refresh_token.insert",
    )
}

pub async fn find_by_hash(handle: &mut Handle<'_>, hash: &str) -> Result<Option<RefreshToken>> {
    sqlx::query_as::<_, TokenRow>(
        "SELECT id, user_id, family_id, token_hash, expires_at, family_expires_at,
                used_at, revoked_at, revoke_reason, created_at
         FROM conbira_refresh_tokens WHERE token_hash = $1",
    )
    .bind(hash)
    .fetch_optional(handle.conn())
    .await
    .map(|row| row.map(Into::into))
    .internal(
        "Failed to find refresh token",
        "auth.repository.refresh_token.find_by_hash",
    )
}

/// 트랜잭션의 handle을 전달해야 잠금이 트랜잭션 종료까지 유지된다.
pub async fn find_by_hash_for_update(
    handle: &mut Handle<'_>,
    hash: &str,
) -> Result<Option<RefreshToken>> {
    sqlx::query_as::<_, TokenRow>(
        "SELECT id, user_id, family_id, token_hash, expires_at, family_expires_at,
                used_at, revoked_at, revoke_reason, created_at
         FROM conbira_refresh_tokens WHERE token_hash = $1 FOR UPDATE",
    )
    .bind(hash)
    .fetch_optional(handle.conn())
    .await
    .map(|row| row.map(Into::into))
    .internal(
        "Failed to lock refresh token",
        "auth.repository.refresh_token.find_by_hash_for_update",
    )
}

pub async fn find_by_family_id(
    handle: &mut Handle<'_>,
    family_id: uuid::Uuid,
) -> Result<Vec<RefreshToken>> {
    sqlx::query_as::<_, TokenRow>(
        "SELECT id, user_id, family_id, token_hash, expires_at, family_expires_at,
                used_at, revoked_at, revoke_reason, created_at
         FROM conbira_refresh_tokens WHERE family_id = $1 ORDER BY id",
    )
    .bind(family_id)
    .fetch_all(handle.conn())
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .internal(
        "Failed to find refresh token family",
        "auth.repository.refresh_token.find_by_family_id",
    )
}

pub async fn update(handle: &mut Handle<'_>, token: &RefreshToken) -> Result<Option<RefreshToken>> {
    sqlx::query_as::<_, TokenRow>(
        "UPDATE conbira_refresh_tokens
         SET used_at = $2, revoked_at = $3, revoke_reason = $4
         WHERE id = $1
         RETURNING id, user_id, family_id, token_hash, expires_at, family_expires_at,
                   used_at, revoked_at, revoke_reason, created_at",
    )
    .bind(token.id)
    .bind(token.used_at.map(jiff_sqlx::Timestamp::from))
    .bind(token.revoked_at.map(jiff_sqlx::Timestamp::from))
    .bind(&token.revoke_reason)
    .fetch_optional(handle.conn())
    .await
    .map(|row| row.map(Into::into))
    .internal(
        "Failed to update refresh token",
        "auth.repository.refresh_token.update",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::container::db::Database;

    async fn create_tables(handle: &mut Handle<'_>, temporary: bool) -> anyhow::Result<i64> {
        for migration in [
            include_str!("../../../../migrations/0010_user.sql"),
            include_str!("../../../../migrations/0011_refresh_token.sql"),
        ] {
            let schema = if temporary {
                migration.replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE")
            } else {
                migration.to_owned()
            };
            sqlx::raw_sql(sqlx::AssertSqlSafe(schema))
                .execute(handle.conn())
                .await?;
        }
        Ok(
            sqlx::query_scalar("INSERT INTO conbira_users (nickname) VALUES ('user') RETURNING id")
                .fetch_one(handle.conn())
                .await?,
        )
    }

    fn new_token(user_id: i64, family_id: uuid::Uuid, hash: &str) -> NewRefreshToken {
        NewRefreshToken {
            user_id,
            family_id,
            token_hash: hash.into(),
            expires_at: jiff::Timestamp::from_second(1_800_000_000).unwrap(),
            family_expires_at: jiff::Timestamp::from_second(1_900_000_000).unwrap(),
        }
    }

    #[tokio::test]
    async fn stores_and_finds_tokens_by_hash_and_family() -> anyhow::Result<()> {
        let db = Database::new(&crate::test_support::database_config(1)).await?;
        let mut connection = db.conn().await?;
        let mut handle = connection.handle();
        let user_id = create_tables(&mut handle, true).await?;
        let family_id = uuid::Uuid::parse_str("edc3c855-e5d4-449c-89c8-90946f212345")?;
        let input = new_token(user_id, family_id, "hash-1");
        let token = insert(&mut handle, &input).await?;
        let second = insert(&mut handle, &new_token(user_id, family_id, "hash-2")).await?;
        insert(
            &mut handle,
            &new_token(user_id, uuid::Uuid::nil(), "other-family"),
        )
        .await?;

        // 발급 정보와 초기 상태가 보존되고, 해시와 계열로 해당 토큰만 조회된다.
        assert_eq!(token.expires_at, input.expires_at);
        assert_eq!(token.family_expires_at, input.family_expires_at);
        assert!(
            token.used_at.is_none() && token.revoked_at.is_none() && token.revoke_reason.is_none()
        );
        assert!(find_by_hash(&mut handle, "hash-1").await? == Some(token));
        let family = find_by_family_id(&mut handle, family_id).await?;
        assert_eq!(family.len(), 2);
        assert_eq!(family[1].id, second.id);
        assert!(find_by_hash(&mut handle, "missing").await?.is_none());
        assert!(
            find_by_family_id(&mut handle, uuid::Uuid::max())
                .await?
                .is_empty()
        );
        // 토큰 해시는 중복 저장할 수 없다.
        assert!(
            insert(&mut handle, &new_token(user_id, family_id, "hash-1"))
                .await
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn updates_usage_and_revocation_without_hiding_token_history() -> anyhow::Result<()> {
        let db = Database::new(&crate::test_support::database_config(1)).await?;
        let mut connection = db.conn().await?;
        let mut handle = connection.handle();
        let user_id = create_tables(&mut handle, true).await?;
        let mut token = insert(&mut handle, &new_token(user_id, uuid::Uuid::nil(), "hash")).await?;
        token.used_at = Some(jiff::Timestamp::from_second(1_700_000_000)?);
        token.revoked_at = Some(jiff::Timestamp::from_second(1_700_000_001)?);
        token.revoke_reason = Some("reuse".into());

        // 사용·취소 기록이 저장되어도 토큰 이력은 해시로 조회된다.
        assert!(update(&mut handle, &token).await?.is_some());
        let stored = find_by_hash(&mut handle, "hash").await?.unwrap();
        assert_eq!(
            stored.used_at,
            Some(jiff::Timestamp::from_second(1_700_000_000)?)
        );
        assert_eq!(
            stored.revoked_at,
            Some(jiff::Timestamp::from_second(1_700_000_001)?)
        );
        assert_eq!(stored.revoke_reason.as_deref(), Some("reuse"));
        let mut missing = stored;
        missing.id = -1;
        assert!(update(&mut handle, &missing).await?.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn locked_token_blocks_another_transaction_until_commit() -> anyhow::Result<()> {
        let db = Database::new(&crate::test_support::database_config(2)).await?;
        let mut first = db.conn().await?;
        let mut second = db.conn().await?;
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(first.handle().conn())
            .await?;
        // 스키마 이름은 서버가 반환한 정수 PID로만 구성한다.
        let schema = format!("refresh_token_test_{pid}");
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(first.handle().conn())
            .await?;
        let test_result: anyhow::Result<()> = async {
            for connection in [&mut first, &mut second] {
                sqlx::query(sqlx::AssertSqlSafe(format!("SET search_path TO {schema}")))
                    .execute(connection.handle().conn())
                    .await?;
            }
            let user_id = create_tables(&mut first.handle(), false).await?;
            insert(
                &mut first.handle(),
                &new_token(user_id, uuid::Uuid::nil(), "hash"),
            )
            .await?;

            let mut first_tx = first.begin().await?;
            anyhow::ensure!(
                find_by_hash_for_update(&mut first_tx.handle(), "hash")
                    .await?
                    .is_some(),
                "첫 트랜잭션에서 토큰을 찾지 못함"
            );
            let mut second_tx = second.begin().await?;
            sqlx::query("SET LOCAL lock_timeout = '100ms'")
                .execute(second_tx.handle().conn())
                .await?;
            // 첫 트랜잭션이 잠근 행은 다른 트랜잭션에서 잠금 시간 초과가
            // 발생한다.
            let error = match find_by_hash_for_update(&mut second_tx.handle(), "hash").await {
                Err(error) => error,
                Ok(_) => anyhow::bail!("잠금 조회가 차단되지 않음"),
            };
            let sql_error = error
                .cause
                .as_ref()
                .and_then(|cause| cause.downcast_ref::<sqlx::Error>());
            let code = sql_error
                .and_then(sqlx::Error::as_database_error)
                .and_then(|error| error.code());
            anyhow::ensure!(
                code.as_deref() == Some("55P03"),
                "잠금 시간 초과 대신 다른 오류 발생: {error}"
            );
            second_tx.rollback().await?;
            first_tx.commit().await?;
            let mut second_tx = second.begin().await?;
            // 첫 트랜잭션을 커밋하면 다른 트랜잭션에서 토큰을 잠글 수 있다.
            anyhow::ensure!(
                find_by_hash_for_update(&mut second_tx.handle(), "hash")
                    .await?
                    .is_some(),
                "커밋 후 토큰을 찾지 못함"
            );
            second_tx.rollback().await?;
            Ok(())
        }
        .await;
        let cleanup_result =
            sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
                .execute(first.handle().conn())
                .await;
        match (test_result, cleanup_result) {
            (Err(test_error), Err(cleanup_error)) => {
                Err(test_error.context(format!("스키마 정리 실패: {cleanup_error}")))
            }
            (Err(test_error), _) => Err(test_error),
            (Ok(()), Err(cleanup_error)) => Err(cleanup_error.into()),
            (Ok(()), Ok(_)) => Ok(()),
        }
    }
}
