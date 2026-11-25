use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand_core::{OsRng, RngCore as _};
use sha2::{Digest as _, Sha256};

use crate::app::auth::entity::RefreshToken;
use crate::app::auth::repository::refresh_token;
use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub ttl_seconds: u64,
    pub absolute_ttl_seconds: u64,
}
pub struct RefreshTokenService {
    config: Config,
}
pub struct IssuedRefreshToken {
    pub token: String,
}

impl RefreshTokenService {
    pub fn new(config: Config) -> Self { Self { config } }
    pub async fn issue(
        &self,
        handle: &mut Handle<'_>,
        user_id: i64,
    ) -> crate::Result<IssuedRefreshToken> {
        let now = jiff::Timestamp::now();
        let absolute_expires_at = expiration(now, self.config.absolute_ttl_seconds)?;
        let expires_at = expiration(now, self.config.ttl_seconds)?.min(absolute_expires_at);
        let mut bytes = [0u8; 32];
        OsRng
            .try_fill_bytes(&mut bytes)
            .map_err_internal("Failed to generate refresh token", crate::here!())?;
        let token = URL_SAFE_NO_PAD.encode(bytes);
        refresh_token::insert(
            handle,
            user_id,
            &token_hash(&token),
            expires_at,
            absolute_expires_at,
        )
        .await?;
        Ok(IssuedRefreshToken { token })
    }
    pub async fn find_valid(
        &self,
        handle: &mut Handle<'_>,
        token: &str,
    ) -> crate::Result<Option<RefreshToken>> {
        let record = refresh_token::find_for_update(handle, &token_hash(token)).await?;
        let now = jiff::Timestamp::now();
        Ok(record.filter(|r| {
            r.revoked_at.is_none() && r.expires_at > now && r.absolute_expires_at > now
        }))
    }
    pub async fn extend_expiration(
        &self,
        handle: &mut Handle<'_>,
        record: &RefreshToken,
        now: jiff::Timestamp,
    ) -> crate::Result<()> {
        let expires_at = expiration(now, self.config.ttl_seconds)?.min(record.absolute_expires_at);
        if expires_at > record.expires_at {
            refresh_token::extend_expiration(handle, record.id, expires_at).await?;
        }
        Ok(())
    }
    pub async fn revoke(&self, handle: &mut Handle<'_>, token: &str) -> crate::Result<()> {
        refresh_token::revoke(handle, &token_hash(token)).await
    }
}
fn expiration(now: jiff::Timestamp, ttl_seconds: u64) -> crate::Result<jiff::Timestamp> {
    let seconds = i64::try_from(ttl_seconds)
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            crate::Error::new(
                crate::Code::InvalidConfiguration,
                "Invalid refresh token lifetime",
            )
            .at(crate::here!())
        })?;
    now.checked_add(jiff::SignedDuration::from_secs(seconds))
        .map_err_internal("Invalid refresh token expiration", crate::here!())
}
fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    mod contract {
        use super::super::*;
        use crate::app::container::db::Database;

        #[tokio::test]
        async fn initial_refresh_token_schema_can_be_applied_twice() -> anyhow::Result<()> {
            let db = Database::new(&crate::test_support::database_config(1)).await?;
            let mut conn = db.conn().await?;
            for migration in [
                include_str!("../../../../migrations/0010_user.sql"),
                include_str!("../../../../migrations/0011_refresh_token.sql"),
                include_str!("../../../../migrations/0011_refresh_token.sql"),
            ] {
                let sql = migration.replace(
                    "CREATE TABLE IF NOT EXISTS",
                    "CREATE TEMPORARY TABLE IF NOT EXISTS",
                );
                sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
                    .execute(conn.handle().conn())
                    .await?;
            }
            Ok(())
        }

        #[tokio::test]
        async fn tokens_can_be_reused_and_logout_revokes_only_one() -> anyhow::Result<()> {
            let db = Database::new(&crate::test_support::database_config(1)).await?;
            let mut conn = db.conn().await?;
            for migration in [
                include_str!("../../../../migrations/0010_user.sql"),
                include_str!("../../../../migrations/0011_refresh_token.sql"),
            ] {
                let sql = migration.replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE");
                sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
                    .execute(conn.handle().conn())
                    .await?;
            }
            let user_id: i64 = sqlx::query_scalar(
                "INSERT INTO conbira_auth_users (nickname) VALUES ('user') RETURNING id",
            )
            .fetch_one(conn.handle().conn())
            .await?;
            let service = RefreshTokenService::new(Config {
                ttl_seconds: 604800,
                absolute_ttl_seconds: 2592000,
            });
            let mut tx = conn.begin().await?;
            let first = service.issue(&mut tx.handle(), user_id).await?;
            let second = service.issue(&mut tx.handle(), user_id).await?;
            tx.commit().await?;
            let mut tx = conn.begin().await?;
            assert!(
                service
                    .find_valid(&mut tx.handle(), &first.token)
                    .await?
                    .is_some()
            );
            assert!(
                service
                    .find_valid(&mut tx.handle(), &first.token)
                    .await?
                    .is_some()
            );
            tx.commit().await?;
            let mut tx = conn.begin().await?;
            service.revoke(&mut tx.handle(), &first.token).await?;
            service.revoke(&mut tx.handle(), &first.token).await?;
            tx.commit().await?;
            let mut tx = conn.begin().await?;
            assert!(
                service
                    .find_valid(&mut tx.handle(), &first.token)
                    .await?
                    .is_none()
            );
            assert!(
                service
                    .find_valid(&mut tx.handle(), &second.token)
                    .await?
                    .is_some()
            );
            tx.rollback().await?;
            Ok(())
        }
    }
}
