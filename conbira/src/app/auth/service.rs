use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand_core::{OsRng, RngCore as _};
use sha2::{Digest as _, Sha256};
use sqlx::Connection as _;

use crate::app::auth::Config;
use crate::app::auth::repository::refresh_token::{self, NewRefreshToken};
use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

/// 인증 방식과 사용자 상태 판단을 제외한 refresh token 수명 관리.
pub struct RefreshTokenService {
    config: Config,
}

/// 호출자에게 한 번 전달하는 refresh token과 만료 정보.
pub struct IssuedRefreshToken {
    pub token: String,
    pub user_id: i64,
    pub expires_at: jiff::Timestamp,
    pub family_expires_at: jiff::Timestamp,
}

/// 계열 취소는 오류가 아닌 처리 결과로 반환해 호출자가 커밋할 수 있게 한다.
pub enum RotateOutcome {
    Rotated(IssuedRefreshToken),
    Invalid,
    FamilyRevoked,
}

impl RefreshTokenService {
    pub fn new(config: Config) -> Self { Self { config } }

    /// 호출자는 트랜잭션의 handle을 전달하고 성공 시 커밋한다.
    pub async fn issue(
        &self,
        handle: &mut Handle<'_>,
        user_id: i64,
    ) -> crate::Result<IssuedRefreshToken> {
        require_transaction(handle)?;
        let now = jiff::Timestamp::now();
        let family_expires_at = add_seconds(now, self.config.refresh_token_family_ttl_seconds)?;
        let expires_at =
            add_seconds(now, self.config.refresh_token_ttl_seconds)?.min(family_expires_at);
        self.insert(
            handle,
            user_id,
            uuid::Uuid::new_v4(),
            expires_at,
            family_expires_at,
        )
        .await
    }

    /// 사용자 상태 확인을 위한 조회이며 이후 회전 시 토큰 상태를 다시 검증한다.
    pub async fn find_user_id(
        &self,
        handle: &mut Handle<'_>,
        token: &str,
    ) -> crate::Result<Option<i64>> {
        Ok(refresh_token::find_by_hash(handle, &token_hash(token))
            .await?
            .map(|record| record.user_id))
    }

    /// 트랜잭션의 handle을 전달한다. FamilyRevoked 결과도 취소를 보존하도록
    /// 커밋한다.
    pub async fn rotate(
        &self,
        handle: &mut Handle<'_>,
        token: &str,
    ) -> crate::Result<RotateOutcome> {
        require_transaction(handle)?;
        let hash = token_hash(token);
        let Some(existing) = refresh_token::find_by_hash(handle, &hash).await? else {
            return Ok(RotateOutcome::Invalid);
        };
        refresh_token::lock_family(handle, existing.family_id).await?;
        let Some(mut record) = refresh_token::find_by_hash_for_update(handle, &hash).await? else {
            return Ok(RotateOutcome::Invalid);
        };
        let now = jiff::Timestamp::now();
        if record.used_at.is_some()
            || record.revoked_at.is_some()
            || record.expires_at <= now
            || record.family_expires_at <= now
        {
            refresh_token::revoke_family(handle, record.family_id, "reuse_detected").await?;
            return Ok(RotateOutcome::FamilyRevoked);
        }
        record.used_at = Some(now);
        refresh_token::update(handle, &record)
            .await?
            .ok_or_else(|| {
                crate::Error::internal("Refresh token disappeared").at("auth.refresh_token.rotate")
            })?;
        let expires_at =
            add_seconds(now, self.config.refresh_token_ttl_seconds)?.min(record.family_expires_at);
        let issued = self
            .insert(
                handle,
                record.user_id,
                record.family_id,
                expires_at,
                record.family_expires_at,
            )
            .await?;
        Ok(RotateOutcome::Rotated(issued))
    }

    /// 트랜잭션의 handle을 전달하고 계열 취소를 커밋한다.
    pub async fn revoke(
        &self,
        handle: &mut Handle<'_>,
        token: &str,
        reason: &str,
    ) -> crate::Result<bool> {
        require_transaction(handle)?;
        let hash = token_hash(token);
        let Some(existing) = refresh_token::find_by_hash(handle, &hash).await? else {
            return Ok(false);
        };
        refresh_token::lock_family(handle, existing.family_id).await?;
        let Some(record) = refresh_token::find_by_hash_for_update(handle, &hash).await? else {
            return Ok(false);
        };
        refresh_token::revoke_family(handle, record.family_id, reason).await?;
        Ok(true)
    }

    async fn insert(
        &self,
        handle: &mut Handle<'_>,
        user_id: i64,
        family_id: uuid::Uuid,
        expires_at: jiff::Timestamp,
        family_expires_at: jiff::Timestamp,
    ) -> crate::Result<IssuedRefreshToken> {
        let mut bytes = [0u8; 32];
        OsRng.try_fill_bytes(&mut bytes).internal(
            "Failed to generate refresh token",
            "auth.refresh_token.issue",
        )?;
        let token = URL_SAFE_NO_PAD.encode(bytes);
        let stored = refresh_token::insert(
            handle,
            &NewRefreshToken {
                user_id,
                family_id,
                token_hash: token_hash(&token),
                expires_at,
                family_expires_at,
            },
        )
        .await?;
        Ok(IssuedRefreshToken {
            token,
            user_id,
            expires_at: stored.expires_at,
            family_expires_at: stored.family_expires_at,
        })
    }
}

fn require_transaction(handle: &mut Handle<'_>) -> crate::Result<()> {
    if !handle.conn().is_in_transaction() {
        return Err(
            crate::Error::internal("Refresh token operation requires a transaction")
                .at("auth.refresh_token.transaction"),
        );
    }
    Ok(())
}

fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn add_seconds(now: jiff::Timestamp, seconds: u64) -> crate::Result<jiff::Timestamp> {
    let seconds = i64::try_from(seconds)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            crate::Error::new(
                crate::Code::InvalidConfiguration,
                "Invalid refresh token lifetime",
            )
            .at("auth.refresh_token.config")
        })?;
    now.checked_add(jiff::SignedDuration::from_secs(seconds))
        .internal(
            "Invalid refresh token expiration",
            "auth.refresh_token.config",
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::container::db::Database;

    fn service() -> RefreshTokenService {
        RefreshTokenService::new(Config {
            refresh_token_ttl_seconds: 3600,
            refresh_token_family_ttl_seconds: 7200,
        })
    }

    async fn setup() -> anyhow::Result<(Database, i64)> {
        let db = Database::new(&crate::test_support::database_config(1)).await?;
        let mut conn = db.conn().await?;
        for migration in [
            include_str!("../../../migrations/0010_user.sql"),
            include_str!("../../../migrations/0011_refresh_token.sql"),
        ] {
            let sql = migration.replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE");
            sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
                .execute(conn.handle().conn())
                .await?;
        }
        let user_id =
            sqlx::query_scalar("INSERT INTO conbira_users (nickname) VALUES ('user') RETURNING id")
                .fetch_one(conn.handle().conn())
                .await?;
        drop(conn);
        Ok((db, user_id))
    }

    #[tokio::test]
    async fn rotation_preserves_family_and_reuse_revokes_only_that_family() -> anyhow::Result<()> {
        let (db, user_id) = setup().await?;
        let service = service();
        let mut tx = db.tx().await?;
        let original = service.issue(&mut tx.handle(), user_id).await?;
        let other = service.issue(&mut tx.handle(), user_id).await?;
        tx.commit().await?;

        let mut tx = db.tx().await?;
        let rotated = match service.rotate(&mut tx.handle(), &original.token).await? {
            RotateOutcome::Rotated(token) => token,
            _ => anyhow::bail!("valid token was not rotated"),
        };
        let stored = refresh_token::find_by_hash(&mut tx.handle(), &token_hash(&original.token))
            .await?
            .unwrap();
        let successor = refresh_token::find_by_hash(&mut tx.handle(), &token_hash(&rotated.token))
            .await?
            .unwrap();
        // 원문은 해시로 저장되고 교체 토큰은 동일 계열과 절대 만료를 유지한다.
        assert_ne!(stored.token_hash, original.token);
        assert_ne!(rotated.token, original.token);
        assert_eq!(stored.family_id, successor.family_id);
        assert_eq!(rotated.family_expires_at, original.family_expires_at);
        assert!(stored.used_at.is_some());
        assert!(rotated.expires_at <= rotated.family_expires_at);
        tx.commit().await?;

        let mut tx = db.tx().await?;
        // 사용된 토큰의 재사용은 계열 취소 결과로 반환되며 호출자가 커밋한다.
        assert!(matches!(
            service.rotate(&mut tx.handle(), &original.token).await?,
            RotateOutcome::FamilyRevoked
        ));
        tx.commit().await?;
        let mut tx = db.tx().await?;
        let successor = refresh_token::find_by_hash(&mut tx.handle(), &token_hash(&rotated.token))
            .await?
            .unwrap();
        let other_record = refresh_token::find_by_hash(&mut tx.handle(), &token_hash(&other.token))
            .await?
            .unwrap();
        assert!(successor.revoked_at.is_some());
        assert_eq!(successor.revoke_reason.as_deref(), Some("reuse_detected"));
        assert!(other_record.revoked_at.is_none());
        assert!(matches!(
            service.rotate(&mut tx.handle(), "unknown").await?,
            RotateOutcome::Invalid
        ));
        tx.commit().await?;
        Ok(())
    }

    #[tokio::test]
    async fn expired_and_revoked_tokens_cancel_their_family() -> anyhow::Result<()> {
        let (db, user_id) = setup().await?;
        let service = service();
        let mut tx = db.tx().await?;
        let expired = service.issue(&mut tx.handle(), user_id).await?;
        let revoked = service.issue(&mut tx.handle(), user_id).await?;
        sqlx::query("UPDATE conbira_refresh_tokens SET expires_at = CURRENT_TIMESTAMP - INTERVAL '1 second' WHERE token_hash = $1")
            .bind(token_hash(&expired.token)).execute(tx.handle().conn()).await?;
        assert!(
            service
                .revoke(&mut tx.handle(), &revoked.token, "logout")
                .await?
        );
        tx.commit().await?;
        let mut tx = db.tx().await?;
        // 만료·취소된 토큰의 제출은 각각 계열 취소로 처리된다.
        assert!(matches!(
            service.rotate(&mut tx.handle(), &expired.token).await?,
            RotateOutcome::FamilyRevoked
        ));
        assert!(matches!(
            service.rotate(&mut tx.handle(), &revoked.token).await?,
            RotateOutcome::FamilyRevoked
        ));
        tx.commit().await?;
        let mut tx = db.tx().await?;
        let expired_row =
            refresh_token::find_by_hash(&mut tx.handle(), &token_hash(&expired.token))
                .await?
                .unwrap();
        let revoked_row =
            refresh_token::find_by_hash(&mut tx.handle(), &token_hash(&revoked.token))
                .await?
                .unwrap();
        assert!(expired_row.revoked_at.is_some());
        assert_eq!(revoked_row.revoke_reason.as_deref(), Some("logout"));
        tx.commit().await?;
        Ok(())
    }

    #[tokio::test]
    async fn state_changes_require_transaction_handle() -> anyhow::Result<()> {
        let (db, user_id) = setup().await?;
        let service = service();
        let mut connection = db.conn().await?;
        let mut handle = connection.handle();
        // 자동 커밋 연결에서는 계열 잠금이 유지되지 않으므로 상태 변경을
        // 거부한다.
        assert!(service.issue(&mut handle, user_id).await.is_err());
        assert!(service.rotate(&mut handle, "unknown").await.is_err());
        assert!(
            service
                .revoke(&mut handle, "unknown", "logout")
                .await
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn rotation_rejects_isolation_that_cannot_observe_prior_family_commit()
    -> anyhow::Result<()> {
        let (db, user_id) = setup().await?;
        let service = service();
        let mut tx = db.tx().await?;
        let token = service.issue(&mut tx.handle(), user_id).await?;
        tx.commit().await?;
        let mut tx = db.tx().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(tx.handle().conn())
            .await?;
        // 대기 중 커밋된 계열 변경을 재조회할 수 없는 격리 수준은 거부한다.
        assert!(
            service
                .rotate(&mut tx.handle(), &token.token)
                .await
                .is_err()
        );
        tx.rollback().await?;
        Ok(())
    }

    #[tokio::test]
    async fn reuse_waits_for_rotation_then_revokes_new_token() -> anyhow::Result<()> {
        let db = Database::new(&crate::test_support::database_config(2)).await?;
        let mut first = db.conn().await?;
        let mut second = db.conn().await?;
        let schema = format!("refresh_service_test_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(first.handle().conn())
            .await?;
        let result: anyhow::Result<()> = async {
            for conn in [&mut first, &mut second] {
                sqlx::query(sqlx::AssertSqlSafe(format!("SET search_path TO {schema}")))
                    .execute(conn.handle().conn())
                    .await?;
            }
            for migration in [
                include_str!("../../../migrations/0010_user.sql"),
                include_str!("../../../migrations/0011_refresh_token.sql"),
            ] {
                sqlx::raw_sql(migration)
                    .execute(first.handle().conn())
                    .await?;
            }
            let user_id: i64 = sqlx::query_scalar(
                "INSERT INTO conbira_users (nickname) VALUES ('user') RETURNING id",
            )
            .fetch_one(first.handle().conn())
            .await?;
            let service = service();
            let mut tx = first.begin().await?;
            let original = service.issue(&mut tx.handle(), user_id).await?;
            tx.commit().await?;
            let mut tx = first.begin().await?;
            let current = match service.rotate(&mut tx.handle(), &original.token).await? {
                RotateOutcome::Rotated(token) => token,
                _ => anyhow::bail!("initial rotation failed"),
            };
            tx.commit().await?;

            let mut rotation_tx = first.begin().await?;
            let next = match service
                .rotate(&mut rotation_tx.handle(), &current.token)
                .await?
            {
                RotateOutcome::Rotated(token) => token,
                _ => anyhow::bail!("second rotation failed"),
            };
            let mut reuse_tx = second.begin().await?;
            {
                let mut handle = reuse_tx.handle();
                let future = service.rotate(&mut handle, &original.token);
                tokio::pin!(future);
                // 이전 토큰 재사용 처리는 같은 계열의 회전 트랜잭션을 기다린다.
                assert!(
                    tokio::time::timeout(std::time::Duration::from_millis(100), &mut future)
                        .await
                        .is_err()
                );
                rotation_tx.commit().await?;
                assert!(matches!(future.await?, RotateOutcome::FamilyRevoked));
            }
            reuse_tx.commit().await?;
            let stored = refresh_token::find_by_hash(&mut first.handle(), &token_hash(&next.token))
                .await?
                .unwrap();
            // 회전 커밋 후 재사용 취소가 새로 발급된 토큰까지 무효화한다.
            assert!(stored.revoked_at.is_some());
            Ok(())
        }
        .await;
        let cleanup = sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
            .execute(first.handle().conn())
            .await;
        match (result, cleanup) {
            (Err(error), Err(cleanup)) => {
                Err(error.context(format!("schema cleanup failed: {cleanup}")))
            }
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error.into()),
            (Ok(()), Ok(_)) => Ok(()),
        }
    }
}
