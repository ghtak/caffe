use uuid::Uuid;

use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

pub async fn list_ids(handle: &mut Handle<'_>, post_id: i64) -> crate::Result<Vec<Uuid>> {
    sqlx::query_scalar("SELECT file_asset_id FROM conbira_post_attachments WHERE post_id = $1 ORDER BY created_at, file_asset_id")
        .bind(post_id).fetch_all(handle.conn()).await.map_err_internal("Failed to list post attachments", crate::here!())
}

pub async fn insert(handle: &mut Handle<'_>, post_id: i64, asset_id: Uuid) -> crate::Result<()> {
    sqlx::query("INSERT INTO conbira_post_attachments (post_id, file_asset_id) VALUES ($1, $2)")
        .bind(post_id)
        .bind(asset_id)
        .execute(handle.conn())
        .await
        .map_err_internal("Failed to attach asset to post", crate::here!())?;
    Ok(())
}

pub async fn remove(handle: &mut Handle<'_>, post_id: i64, asset_id: Uuid) -> crate::Result<bool> {
    let result = sqlx::query(
        "DELETE FROM conbira_post_attachments WHERE post_id = $1 AND file_asset_id = $2",
    )
    .bind(post_id)
    .bind(asset_id)
    .execute(handle.conn())
    .await
    .map_err_internal("Failed to detach post asset", crate::here!())?;
    Ok(result.rows_affected() == 1)
}

pub async fn is_downloadable(
    handle: &mut Handle<'_>,
    post_id: i64,
    asset_id: Uuid,
) -> crate::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM conbira_post_attachments a JOIN conbira_posts p ON p.id = a.post_id JOIN conbira_file_assets f ON f.id = a.file_asset_id WHERE a.post_id = $1 AND a.file_asset_id = $2 AND p.deleted_at IS NULL AND f.status = 'attached')")
        .bind(post_id).bind(asset_id).fetch_one(handle.conn()).await
        .map_err_internal("Failed to find post attachment", crate::here!())
}
