use crate::app::board::entity::{Comment, CommentSummary};
use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

pub async fn list(handle: &mut Handle<'_>, post_id: i64) -> crate::Result<Vec<CommentSummary>> {
    sqlx::query_as("SELECT c.*, u.nickname AS author_nickname FROM conbira_post_comments c JOIN conbira_posts p ON p.id = c.post_id AND p.deleted_at IS NULL JOIN conbira_auth_users u ON u.id = c.author_id LEFT JOIN conbira_post_comments parent ON parent.id = c.parent_id WHERE c.post_id = $1 AND c.deleted_at IS NULL AND c.is_secret = FALSE AND (c.parent_id IS NULL OR parent.is_secret = FALSE) ORDER BY COALESCE(c.parent_id, c.id), c.parent_id NULLS FIRST, c.created_at, c.id")
        .bind(post_id).fetch_all(handle.conn()).await
        .map_err_internal("Failed to list comments", crate::here!())
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
