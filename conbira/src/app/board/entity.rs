#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct Post {
    pub id: i64,
    pub board_slug: String,
    pub category: Option<String>,
    pub author_id: i64,
    pub title: String,
    pub excerpt: Option<String>,
    pub attachment_group_id: Option<uuid::Uuid>,
    pub thumbnail_file_id: Option<uuid::Uuid>,
    pub is_notice: bool,
    pub is_secret: bool,
    pub qna_status: Option<String>,
    pub accepted_comment_id: Option<i64>,
    pub view_count: i64,
    pub like_count: i32,
    pub comment_count: i32,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub created_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub updated_at: jiff::Timestamp,
}
