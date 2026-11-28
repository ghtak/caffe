use crate::app::board::entity::{Comment, CommentSummary};
use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

pub async fn update_content(
    handle: &mut Handle<'_>,
    id: i64,
    author_id: i64,
    content: &str,
) -> crate::Result<Option<Comment>> {
    sqlx::query_as("UPDATE conbira_post_comments c SET content = $3, updated_at = CURRENT_TIMESTAMP WHERE c.id = $1 AND c.author_id = $2 AND c.deleted_at IS NULL AND EXISTS (SELECT 1 FROM conbira_posts p WHERE p.id = c.post_id AND p.deleted_at IS NULL) RETURNING c.*")
        .bind(id).bind(author_id).bind(content).fetch_optional(handle.conn()).await
        .map_err_internal("Failed to update comment", crate::here!())
}

pub async fn list(handle: &mut Handle<'_>, post_id: i64) -> crate::Result<Vec<CommentSummary>> {
    sqlx::query_as("SELECT c.id, c.post_id, c.parent_id, c.author_id, CASE WHEN c.deleted_at IS NULL THEN c.content ELSE '' END AS content, c.is_secret, c.created_at, c.updated_at, u.nickname AS author_nickname, (c.deleted_at IS NOT NULL) AS is_deleted FROM conbira_post_comments c JOIN conbira_posts p ON p.id = c.post_id AND p.deleted_at IS NULL JOIN conbira_auth_users u ON u.id = c.author_id LEFT JOIN conbira_post_comments parent ON parent.id = c.parent_id WHERE c.post_id = $1 AND c.is_secret = FALSE AND (c.parent_id IS NULL OR parent.is_secret = FALSE) ORDER BY COALESCE(c.parent_id, c.id), c.parent_id NULLS FIRST, c.created_at, c.id")
        .bind(post_id).fetch_all(handle.conn()).await
        .map_err_internal("Failed to list comments", crate::here!())
}

pub async fn delete(handle: &mut Handle<'_>, id: i64, author_id: i64) -> crate::Result<bool> {
    let result = sqlx::query("WITH deleted AS (UPDATE conbira_post_comments c SET deleted_at = CURRENT_TIMESTAMP WHERE c.id = $1 AND c.author_id = $2 AND c.deleted_at IS NULL AND EXISTS (SELECT 1 FROM conbira_posts p WHERE p.id = c.post_id AND p.deleted_at IS NULL) RETURNING c.post_id) UPDATE conbira_posts p SET comment_count = p.comment_count - 1 FROM deleted WHERE p.id = deleted.post_id")
        .bind(id).bind(author_id).execute(handle.conn()).await
        .map_err_internal("Failed to delete comment", crate::here!())?;
    Ok(result.rows_affected() > 0)
}

pub async fn is_active_root(
    handle: &mut Handle<'_>,
    post_id: i64,
    parent_id: i64,
) -> crate::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM conbira_post_comments WHERE id = $1 AND post_id = $2 AND parent_id IS NULL AND deleted_at IS NULL)")
        .bind(parent_id).bind(post_id).fetch_one(handle.conn()).await
        .map_err_internal("Failed to find parent comment", crate::here!())
}

pub async fn insert(
    handle: &mut Handle<'_>,
    post_id: i64,
    parent_id: Option<i64>,
    author_id: i64,
    content: &str,
) -> crate::Result<Comment> {
    sqlx::query_as("INSERT INTO conbira_post_comments (post_id, parent_id, author_id, content) VALUES ($1, $2, $3, $4) RETURNING *")
        .bind(post_id).bind(parent_id).bind(author_id).bind(content).fetch_one(handle.conn()).await
        .map_err_internal("Failed to insert comment", crate::here!())
}
