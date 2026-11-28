use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

pub async fn insert(handle: &mut Handle<'_>, post_id: i64, content: &str) -> crate::Result<()> {
    sqlx::query("INSERT INTO conbira_post_contents (post_id, content) VALUES ($1, $2)")
        .bind(post_id)
        .bind(content)
        .execute(handle.conn())
        .await
        .map_err_internal("Failed to insert post content", crate::here!())?;
    Ok(())
}

pub async fn update(handle: &mut Handle<'_>, post_id: i64, content: &str) -> crate::Result<()> {
    sqlx::query("UPDATE conbira_post_contents SET content = $2, updated_at = CURRENT_TIMESTAMP WHERE post_id = $1 RETURNING post_id")
        .bind(post_id).bind(content).fetch_one(handle.conn()).await
        .map_err_internal("Failed to update post content", crate::here!())?;
    Ok(())
}
