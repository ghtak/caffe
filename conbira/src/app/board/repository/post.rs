use crate::app::board::entity::Post;
use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

pub struct Criteria<'a> {
    pub search: Option<&'a str>,
    pub author_id: Option<i64>,
    pub board_slug: &'a str,
    pub limit: i64,
    pub offset: i64,
}

pub async fn search(handle: &mut Handle<'_>, criteria: &Criteria<'_>) -> crate::Result<Vec<Post>> {
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT p.* FROM conbira_posts p WHERE p.deleted_at IS NULL",
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
        .build_query_as::<Post>()
        .fetch_all(handle.conn())
        .await
        .map_err_internal("Failed to search posts", crate::here!())
}
