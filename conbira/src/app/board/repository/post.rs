use crate::app::board::entity::{Post, PostSummary};
use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

pub struct Criteria<'a> {
    pub search: Option<&'a str>,
    pub author_id: Option<i64>,
    pub board_slug: &'a str,
    pub limit: i64,
    pub offset: i64,
}

pub async fn search(
    handle: &mut Handle<'_>,
    criteria: &Criteria<'_>,
) -> crate::Result<Vec<PostSummary>> {
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT p.*, u.nickname AS author_nickname FROM conbira_posts p JOIN conbira_auth_users u ON u.id = p.author_id WHERE p.deleted_at IS NULL",
    );
    if let Some(search) = criteria.search {
        let pattern = format!("%{search}%");
        query.push(" AND (p.title LIKE ").push_bind(pattern.clone())
            .push(" OR EXISTS (SELECT 1 FROM conbira_post_contents c WHERE c.post_id = p.id AND c.content LIKE ")
            .push_bind(pattern).push("))");
    }
    if let Some(author_id) = criteria.author_id {
        query.push(" AND p.author_id = ").push_bind(author_id);
    }
    query
        .push(" AND p.board_slug = ")
        .push_bind(criteria.board_slug);
    query
        .push(" ORDER BY p.created_at DESC, p.id DESC LIMIT ")
        .push_bind(criteria.limit)
        .push(" OFFSET ")
        .push_bind(criteria.offset);
    query
        .build_query_as::<PostSummary>()
        .fetch_all(handle.conn())
        .await
        .map_err_internal("Failed to search posts", crate::here!())
}

pub struct NewPost {
    pub board_slug: String,
    pub author_id: i64,
    pub title: String,
}

pub async fn increment_comment_count(handle: &mut Handle<'_>, id: i64) -> crate::Result<bool> {
    let result = sqlx::query("UPDATE conbira_posts SET comment_count = comment_count + 1 WHERE id = $1 AND deleted_at IS NULL")
        .bind(id).execute(handle.conn()).await
        .map_err_internal("Failed to update comment count", crate::here!())?;
    Ok(result.rows_affected() == 1)
}

pub async fn insert(handle: &mut Handle<'_>, post: &NewPost) -> crate::Result<Post> {
    sqlx::query_as::<_, Post>(
        "INSERT INTO conbira_posts (board_slug, author_id, title) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(&post.board_slug)
    .bind(post.author_id)
    .bind(&post.title)
    .fetch_one(handle.conn())
    .await
    .map_err_internal("Failed to insert post", crate::here!())
}

pub async fn update_title(
    handle: &mut Handle<'_>,
    id: i64,
    author_id: i64,
    title: &str,
) -> crate::Result<Option<Post>> {
    sqlx::query_as::<_, Post>("UPDATE conbira_posts SET title = $3, updated_at = CURRENT_TIMESTAMP WHERE id = $1 AND author_id = $2 AND deleted_at IS NULL RETURNING *")
        .bind(id).bind(author_id).bind(title).fetch_optional(handle.conn()).await
        .map_err_internal("Failed to update post", crate::here!())
}

pub async fn find_by_id(
    handle: &mut Handle<'_>,
    id: i64,
) -> crate::Result<Option<super::super::entity::PostDetail>> {
    sqlx::query_as("SELECT p.*, u.nickname AS author_nickname, c.content FROM conbira_posts p JOIN conbira_auth_users u ON u.id = p.author_id JOIN conbira_post_contents c ON c.post_id = p.id WHERE p.id = $1 AND p.deleted_at IS NULL")
        .bind(id).fetch_optional(handle.conn()).await
        .map_err_internal("Failed to find post", crate::here!())
}
