#[derive(Debug, PartialEq, sqlx::FromRow)]
pub struct Sample {
    pub id: i64,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
