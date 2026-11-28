use uuid::Uuid;

use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

pub struct NewAsset<'a> {
    pub id: Uuid,
    pub uploaded_by: i64,
    pub bucket: &'a str,
    pub object_key: &'a str,
    pub filename: &'a str,
    pub size_bytes: i64,
    pub content_type: &'a str,
}

pub async fn insert(handle: &mut Handle<'_>, asset: &NewAsset<'_>) -> crate::Result<()> {
    sqlx::query("INSERT INTO conbira_file_assets (id, status, uploaded_by, s3_bucket, s3_object_key, original_filename, size_bytes, content_type) VALUES ($1, 'pending', $2, $3, $4, $5, $6, $7)")
        .bind(asset.id).bind(asset.uploaded_by).bind(asset.bucket).bind(asset.object_key).bind(asset.filename).bind(asset.size_bytes).bind(asset.content_type)
        .execute(handle.conn()).await.map_err_internal("Failed to insert file asset", crate::here!())?;
    Ok(())
}

#[derive(sqlx::FromRow)]
pub struct DownloadAsset {
    pub s3_bucket: String,
    pub s3_object_key: String,
    pub original_filename: String,
}

pub async fn find_download_asset(
    handle: &mut Handle<'_>,
    id: Uuid,
) -> crate::Result<Option<DownloadAsset>> {
    sqlx::query_as("SELECT s3_bucket, s3_object_key, original_filename FROM conbira_file_assets WHERE id = $1 AND status IN ('pending', 'attached')")
        .bind(id).fetch_optional(handle.conn()).await
        .map_err_internal("Failed to find file asset", crate::here!())
}

pub async fn change_status(
    handle: &mut Handle<'_>,
    id: Uuid,
    expected: &str,
    target: &str,
) -> crate::Result<bool> {
    let result = sqlx::query("UPDATE conbira_file_assets SET status = $3, updated_at = CASE WHEN status <> $3 THEN CURRENT_TIMESTAMP ELSE updated_at END, deleted_at = CASE WHEN $3 = 'deleted' THEN COALESCE(deleted_at, CURRENT_TIMESTAMP) ELSE NULL END WHERE id = $1 AND status = $2")
        .bind(id).bind(expected).bind(target).execute(handle.conn()).await
        .map_err_internal("Failed to update file asset status", crate::here!())?;
    Ok(result.rows_affected() == 1)
}

pub async fn attach_owned(handle: &mut Handle<'_>, id: Uuid, user_id: i64) -> crate::Result<bool> {
    let result = sqlx::query("UPDATE conbira_file_assets SET status = 'attached', updated_at = CURRENT_TIMESTAMP WHERE id = $1 AND uploaded_by = $2 AND status = 'pending'")
        .bind(id).bind(user_id).execute(handle.conn()).await
        .map_err_internal("Failed to attach file asset", crate::here!())?;
    Ok(result.rows_affected() == 1)
}

pub async fn mark_deleted(handle: &mut Handle<'_>, id: Uuid) -> crate::Result<bool> {
    let result = sqlx::query("UPDATE conbira_file_assets SET status = 'deleted', updated_at = CASE WHEN status <> 'deleted' THEN CURRENT_TIMESTAMP ELSE updated_at END, deleted_at = COALESCE(deleted_at, CURRENT_TIMESTAMP) WHERE id = $1 AND status IN ('pending', 'attached', 'deleted')")
        .bind(id)
        .execute(handle.conn())
        .await
        .map_err_internal("Failed to delete file asset", crate::here!())?;
    Ok(result.rows_affected() == 1)
}

#[derive(sqlx::FromRow)]
pub struct CleanupAsset {
    pub s3_bucket: String,
    pub s3_object_key: String,
}

pub async fn cleanup_candidates(
    handle: &mut Handle<'_>,
    pending_secs: i64,
    deleted_secs: i64,
) -> crate::Result<Vec<Uuid>> {
    sqlx::query_scalar("SELECT id FROM conbira_file_assets WHERE (status = 'pending' AND created_at <= CURRENT_TIMESTAMP - $1 * INTERVAL '1 second') OR (status = 'deleted' AND deleted_at <= CURRENT_TIMESTAMP - $2 * INTERVAL '1 second') ORDER BY id")
        .bind(pending_secs as f64).bind(deleted_secs as f64).fetch_all(handle.conn()).await
        .map_err_internal("Failed to find cleanup candidates", crate::here!())
}

pub async fn lock_cleanup_candidate(
    handle: &mut Handle<'_>,
    id: Uuid,
    pending_secs: i64,
    deleted_secs: i64,
) -> crate::Result<Option<CleanupAsset>> {
    sqlx::query_as("SELECT s3_bucket, s3_object_key FROM conbira_file_assets WHERE id = $1 AND ((status = 'pending' AND created_at <= CURRENT_TIMESTAMP - $2 * INTERVAL '1 second') OR (status = 'deleted' AND deleted_at <= CURRENT_TIMESTAMP - $3 * INTERVAL '1 second')) FOR UPDATE SKIP LOCKED")
        .bind(id).bind(pending_secs as f64).bind(deleted_secs as f64).fetch_optional(handle.conn()).await
        .map_err_internal("Failed to lock cleanup candidate", crate::here!())
}

pub async fn remove(handle: &mut Handle<'_>, id: Uuid) -> crate::Result<()> {
    sqlx::query("DELETE FROM conbira_file_assets WHERE id = $1")
        .bind(id)
        .execute(handle.conn())
        .await
        .map_err_internal("Failed to remove file asset", crate::here!())?;
    Ok(())
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct FileAssetSummary {
    pub id: Uuid,
    pub status: String,
    pub uploaded_by: i64,
    pub original_filename: String,
    pub size_bytes: i64,
    pub content_type: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub created_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub updated_at: jiff::Timestamp,
}

/// Returns existing assets for the supplied IDs, including deleted assets.
/// Results are not guaranteed to follow the input order.
pub async fn find_by_ids(
    handle: &mut Handle<'_>,
    ids: &[Uuid],
) -> crate::Result<Vec<FileAssetSummary>> {
    sqlx::query_as("SELECT id, status, uploaded_by, original_filename, size_bytes, content_type, created_at, updated_at FROM conbira_file_assets WHERE id = ANY($1)")
        .bind(ids).fetch_all(handle.conn()).await
        .map_err_internal("Failed to find file assets", crate::here!())
}

pub async fn list(
    handle: &mut Handle<'_>,
    limit: i64,
    offset: i64,
) -> crate::Result<Vec<FileAssetSummary>> {
    sqlx::query_as("SELECT id, status, uploaded_by, original_filename, size_bytes, content_type, created_at, updated_at FROM conbira_file_assets ORDER BY created_at DESC, id DESC LIMIT $1 OFFSET $2")
        .bind(limit).bind(offset).fetch_all(handle.conn()).await
        .map_err_internal("Failed to list file assets", crate::here!())
}

#[cfg(test)]
mod tests {
    mod contract {
        #[tokio::test]
        async fn finds_existing_assets_by_id_including_deleted() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0014_file_asset.sql")).execute(conn.handle().conn()).await?;
                let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('reader') RETURNING id").fetch_one(conn.handle().conn()).await?;
                let wanted = uuid::Uuid::new_v4();
                let deleted = uuid::Uuid::new_v4();
                let other = uuid::Uuid::new_v4();
                for (id, status) in [(wanted, "attached"), (deleted, "deleted"), (other, "pending")] {
                    sqlx::query("INSERT INTO conbira_file_assets (id, status, uploaded_by, s3_bucket, s3_object_key, original_filename, size_bytes, content_type) VALUES ($1, $2, $3, 'bucket', $1::text, 'report.txt', 4, 'text/plain')").bind(id).bind(status).bind(user).execute(conn.handle().conn()).await?;
                }
                let found = crate::app::file_asset::find_by_ids(&mut conn.handle(), &[wanted, deleted, uuid::Uuid::new_v4(), wanted]).await?;
                assert_eq!(found.len(), 2);
                assert!(found.iter().any(|asset| asset.id == wanted && asset.original_filename == "report.txt" && asset.status == "attached"));
                assert!(found.iter().any(|asset| asset.id == deleted && asset.status == "deleted"));
                assert!(crate::app::file_asset::find_by_ids(&mut conn.handle(), &[]).await?.is_empty());
                Ok(())
            }).await
        }
    }
}
