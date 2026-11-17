#[allow(dead_code)]
#[derive(Debug, PartialEq, sqlx::FromRow)]
pub struct Sample {
    pub id: i64,
    pub name: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub created_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub updated_at: jiff::Timestamp,
}
