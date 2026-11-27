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
