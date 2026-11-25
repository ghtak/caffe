use crate::app::auth::entity::RefreshToken;
use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

#[derive(sqlx::FromRow)]
struct TokenRow {
    id: i64,
    user_id: i64,
    token_hash: String,
    expires_at: jiff_sqlx::Timestamp,
    absolute_expires_at: jiff_sqlx::Timestamp,
    revoked_at: Option<jiff_sqlx::Timestamp>,
    revoke_reason: Option<String>,
    created_at: jiff_sqlx::Timestamp,
}
impl From<TokenRow> for RefreshToken {
    fn from(row: TokenRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            token_hash: row.token_hash,
            expires_at: row.expires_at.into(),
            absolute_expires_at: row.absolute_expires_at.into(),
            revoked_at: row.revoked_at.map(Into::into),
            revoke_reason: row.revoke_reason,
            created_at: row.created_at.into(),
        }
    }
}

pub async fn insert(
    handle: &mut Handle<'_>,
    user_id: i64,
    hash: &str,
    expires_at: jiff::Timestamp,
    absolute_expires_at: jiff::Timestamp,
) -> crate::Result<()> {
    sqlx::query("INSERT INTO conbira_auth_refresh_tokens (user_id, token_hash, expires_at, absolute_expires_at) VALUES ($1, $2, $3, $4)")
        .bind(user_id).bind(hash).bind(jiff_sqlx::Timestamp::from(expires_at)).bind(jiff_sqlx::Timestamp::from(absolute_expires_at))
        .execute(handle.conn()).await.map_err_internal("Failed to insert refresh token", crate::here!())?;
    Ok(())
}

pub async fn find_for_update(
    handle: &mut Handle<'_>,
    hash: &str,
) -> crate::Result<Option<RefreshToken>> {
    sqlx::query_as::<_, TokenRow>("SELECT id, user_id, token_hash, expires_at, absolute_expires_at, revoked_at, revoke_reason, created_at FROM conbira_auth_refresh_tokens WHERE token_hash = $1 FOR UPDATE")
        .bind(hash).fetch_optional(handle.conn()).await
        .map(|row| row.map(Into::into))
        .map_err_internal("Failed to lock refresh token", crate::here!())
}

pub async fn revoke(handle: &mut Handle<'_>, hash: &str) -> crate::Result<()> {
    sqlx::query("UPDATE conbira_auth_refresh_tokens SET revoked_at = CURRENT_TIMESTAMP, revoke_reason = 'logout' WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > CURRENT_TIMESTAMP")
        .bind(hash).execute(handle.conn()).await.map_err_internal("Failed to revoke refresh token", crate::here!())?;
    Ok(())
}

pub async fn extend_expiration(
    handle: &mut Handle<'_>,
    id: i64,
    expires_at: jiff::Timestamp,
) -> crate::Result<()> {
    sqlx::query("UPDATE conbira_auth_refresh_tokens SET expires_at = $1 WHERE id = $2")
        .bind(jiff_sqlx::Timestamp::from(expires_at))
        .bind(id)
        .execute(handle.conn())
        .await
        .map_err_internal("Failed to extend refresh token expiration", crate::here!())?;
    Ok(())
}
