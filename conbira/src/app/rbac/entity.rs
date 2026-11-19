#[derive(Debug, PartialEq, serde::Serialize, sqlx::FromRow)]
pub struct Role {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, PartialEq, serde::Serialize, sqlx::FromRow)]
pub struct Permission {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub description: String,
}
