#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum UserStatus {
    Registered,
    Active,
    Inactive,
    Withdrawn,
}

#[derive(Debug, serde::Serialize)]
pub struct UserInfo {
    pub id: i64,
    pub nickname: String,
    pub status: UserStatus,
    pub email: String,
    pub created_at: jiff::Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum Provider {
    Local,
    Google,
}

#[derive(Debug, PartialEq, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub nickname: String,
    pub status: UserStatus,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub created_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub updated_at: jiff::Timestamp,
}

#[derive(PartialEq, sqlx::FromRow)]
pub struct Identity {
    pub id: i64,
    pub user_id: i64,
    pub provider: Provider,
    pub provider_subject: String,
    pub email: String,
    pub password_hash: Option<String>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub created_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub updated_at: jiff::Timestamp,
}

#[derive(PartialEq)]
pub struct RefreshToken {
    pub id: i64,
    pub user_id: i64,
    pub family_id: uuid::Uuid,
    pub token_hash: String,
    pub expires_at: jiff::Timestamp,
    pub family_expires_at: jiff::Timestamp,
    pub used_at: Option<jiff::Timestamp>,
    pub revoked_at: Option<jiff::Timestamp>,
    pub revoke_reason: Option<String>,
    pub created_at: jiff::Timestamp,
}
