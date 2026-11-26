use crate::Result;
use crate::app::auth::entity::{User, UserStatus};
use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

pub struct NewUser {
    pub nickname: String,
    pub status: UserStatus,
}

pub async fn insert(handle: &mut Handle<'_>, user: &NewUser) -> Result<User> {
    sqlx::query_as::<_, User>(
        "INSERT INTO conbira_auth_users (nickname, status)
         VALUES ($1, $2)
         RETURNING id, nickname, status, created_at, updated_at",
    )
    .bind(&user.nickname)
    .bind(user.status)
    .fetch_one(handle.conn())
    .await
    .map_err_internal("Failed to insert user", crate::here!())
}

pub async fn find_by_id(handle: &mut Handle<'_>, id: i64) -> Result<Option<User>> {
    sqlx::query_as::<_, User>(
        "SELECT id, nickname, status, created_at, updated_at
         FROM conbira_auth_users
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(handle.conn())
    .await
    .map_err_internal("Failed to find user", crate::here!())
}

pub struct Criteria<'a> {
    pub name: Option<&'a str>,
    pub email: Option<&'a str>,
    pub status: Option<UserStatus>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(sqlx::FromRow)]
pub struct SearchUser {
    pub id: i64,
    pub nickname: String,
    pub status: UserStatus,
    pub emails: Vec<String>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub created_at: jiff::Timestamp,
}

pub async fn search(handle: &mut Handle<'_>, criteria: &Criteria<'_>) -> Result<Vec<SearchUser>> {
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT u.id, u.nickname, u.status, u.created_at,
         ARRAY(SELECT DISTINCT i.email FROM conbira_auth_user_identities i
               WHERE i.user_id = u.id ORDER BY i.email) AS emails
         FROM conbira_auth_users u WHERE TRUE",
    );
    if let Some(name) = criteria.name {
        query
            .push(" AND strpos(lower(nickname), lower(")
            .push_bind(name)
            .push(")) > 0");
    }
    if let Some(email) = criteria.email {
        query.push(" AND EXISTS (SELECT 1 FROM conbira_auth_user_identities i WHERE i.user_id = u.id AND strpos(lower(i.email), lower(")
            .push_bind(email).push(")) > 0)");
    }
    if let Some(status) = criteria.status {
        query.push(" AND status = ").push_bind(status);
    }
    query
        .push(" ORDER BY id LIMIT ")
        .push_bind(criteria.limit)
        .push(" OFFSET ")
        .push_bind(criteria.offset);
    query
        .build_query_as::<SearchUser>()
        .fetch_all(handle.conn())
        .await
        .map_err_internal("Failed to search users", crate::here!())
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct DetailUser {
    pub id: i64,
    pub nickname: String,
    pub emails: Vec<String>,
    pub status: UserStatus,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub created_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub updated_at: jiff::Timestamp,
}

pub async fn find_detail_by_id(handle: &mut Handle<'_>, id: i64) -> Result<Option<DetailUser>> {
    sqlx::query_as::<_, DetailUser>(
        "SELECT u.id, u.nickname, u.status, u.created_at, u.updated_at,
         ARRAY(SELECT DISTINCT i.email FROM conbira_auth_user_identities i
               WHERE i.user_id = u.id ORDER BY i.email) AS emails
         FROM conbira_auth_users u WHERE u.id = $1",
    )
    .bind(id)
    .fetch_optional(handle.conn())
    .await
    .map_err_internal("Failed to read user detail", crate::here!())
}

pub async fn update(handle: &mut Handle<'_>, user: &User) -> Result<Option<User>> {
    sqlx::query_as::<_, User>(
        "UPDATE conbira_auth_users
         SET nickname = $2, status = $3, updated_at = CURRENT_TIMESTAMP
         WHERE id = $1
         RETURNING id, nickname, status, created_at, updated_at",
    )
    .bind(user.id)
    .bind(&user.nickname)
    .bind(user.status)
    .fetch_optional(handle.conn())
    .await
    .map_err_internal("Failed to update user", crate::here!())
}

pub async fn update_status(handle: &mut Handle<'_>, id: i64, status: UserStatus) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE conbira_auth_users SET status = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
    )
    .bind(id)
    .bind(status)
    .execute(handle.conn())
    .await
    .map_err_internal("Failed to update user status", crate::here!())?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    mod contract {
        use super::super::*;
        use crate::app::auth::entity::UserStatus;
        use crate::app::container::db::Database;

        #[tokio::test]
        async fn creates_reads_and_updates_user_with_status_and_timestamps() -> anyhow::Result<()> {
            let database = Database::new(&crate::test_support::database_config(1)).await?;
            let mut connection = database.conn().await?;
            let schema = include_str!("../../../../migrations/0010_user.sql")
                .split_once(';')
                .unwrap()
                .0
                .replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE");
            let mut handle = connection.handle();
            sqlx::query(sqlx::AssertSqlSafe(schema))
                .execute(handle.conn())
                .await?;

            let mut user = insert(
                &mut handle,
                &NewUser {
                    nickname: "first".into(),
                    status: UserStatus::Registered,
                },
            )
            .await?;
            // 생성된 사용자는 기본 상태와 같은 생성·수정 시각을 가진다.
            assert_eq!(user.status, UserStatus::Registered);
            assert_eq!(user.created_at, user.updated_at);
            assert_eq!(
                find_by_id(&mut handle, user.id).await?,
                Some(User {
                    id: user.id,
                    nickname: "first".into(),
                    status: UserStatus::Registered,
                    created_at: user.created_at,
                    updated_at: user.updated_at,
                })
            );

            let old_timestamp = jiff::Timestamp::from_second(1_700_000_000)?;
            sqlx::query("UPDATE conbira_auth_users SET updated_at = $2 WHERE id = $1")
                .bind(user.id)
                .bind(jiff_sqlx::Timestamp::from(old_timestamp))
                .execute(handle.conn())
                .await?;
            user.nickname = "updated".into();
            user.status = UserStatus::Active;
            let updated = update(&mut handle, &user).await?.unwrap();
            // 닉네임과 상태가 저장되고 생성 시각은 보존되며 수정 시각은
            // 갱신된다.
            assert_eq!(updated.nickname, "updated");
            assert_eq!(updated.status, UserStatus::Active);
            assert_eq!(updated.created_at, user.created_at);
            assert!(updated.updated_at > old_timestamp);
            assert_eq!(find_by_id(&mut handle, user.id).await?, Some(updated));

            user.id = -1;
            // 없는 사용자는 조회와 수정 모두 결과가 없다.
            assert_eq!(find_by_id(&mut handle, user.id).await?, None);
            assert_eq!(update(&mut handle, &user).await?, None);
            Ok(())
        }
    }
}
