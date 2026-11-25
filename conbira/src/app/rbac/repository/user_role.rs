use crate::Result;
use crate::app::container::db::Handle;
use crate::app::rbac::entity::Role;
use crate::error::ResultExt as _;

pub async fn insert(handle: &mut Handle<'_>, user_id: i64, role_id: i64) -> Result<()> {
    sqlx::query("INSERT INTO conbira_rbac_user_roles (user_id, role_id) VALUES ($1, $2)")
        .bind(user_id)
        .bind(role_id)
        .execute(handle.conn())
        .await
        .map_err(|error| {
            let code = error.as_database_error();
            let (kind, message) = if code.is_some_and(|e| e.is_unique_violation()) {
                (crate::Code::InvalidRequest, "User already has role")
            } else if code.is_some_and(|e| e.is_foreign_key_violation()) {
                (crate::Code::NotFound, "User or role not found")
            } else {
                return crate::Error::internal("Failed to insert user_role")
                    .at(crate::here!())
                    .cause(error);
            };
            crate::Error::new(kind, message)
                .at(crate::here!())
                .cause(error)
        })?;
    Ok(())
}

pub async fn list_by_user_id(handle: &mut Handle<'_>, user_id: i64) -> Result<Vec<Role>> {
    sqlx::query_as::<_, Role>(
        "SELECT e.id, e.code, e.name, e.description FROM conbira_rbac_roles e
         JOIN conbira_rbac_user_roles link ON link.role_id = e.id
         WHERE link.user_id = $1 ORDER BY e.id",
    )
    .bind(user_id)
    .fetch_all(handle.conn())
    .await
    .map_err_internal("Failed to list user_roles", crate::here!())
}

pub async fn search_by_role_id(
    handle: &mut Handle<'_>,
    role_id: i64,
    criteria: &crate::app::auth::repository::user::Criteria<'_>,
) -> Result<Vec<crate::app::auth::repository::user::SearchUser>> {
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT u.id, u.nickname, u.status, u.created_at,
         ARRAY(SELECT DISTINCT i.email FROM conbira_auth_user_identities i WHERE i.user_id = u.id ORDER BY i.email) AS emails
         FROM conbira_auth_users u JOIN conbira_rbac_user_roles link ON link.user_id = u.id WHERE link.role_id = ");
    query.push_bind(role_id);
    if let Some(name) = criteria.name {
        query
            .push(" AND strpos(lower(u.nickname), lower(")
            .push_bind(name)
            .push(")) > 0");
    }
    if let Some(email) = criteria.email {
        query.push(" AND EXISTS (SELECT 1 FROM conbira_auth_user_identities i WHERE i.user_id = u.id AND strpos(lower(i.email), lower(").push_bind(email).push(")) > 0)");
    }
    if let Some(status) = criteria.status {
        query.push(" AND u.status = ").push_bind(status);
    }
    query
        .push(" ORDER BY u.id LIMIT ")
        .push_bind(criteria.limit)
        .push(" OFFSET ")
        .push_bind(criteria.offset);
    query
        .build_query_as::<crate::app::auth::repository::user::SearchUser>()
        .fetch_all(handle.conn())
        .await
        .map_err_internal("Failed to search role users", crate::here!())
}

#[derive(sqlx::FromRow)]
pub struct RoleCandidate {
    pub id: i64,
    pub nickname: String,
    pub emails: Vec<String>,
    pub status: crate::app::auth::entity::UserStatus,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub created_at: jiff::Timestamp,
    pub assigned: bool,
}

pub async fn search_candidates(
    handle: &mut Handle<'_>,
    role_id: i64,
    criteria: &crate::app::auth::repository::user::Criteria<'_>,
) -> Result<Vec<RoleCandidate>> {
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT u.id, u.nickname, u.status, u.created_at,
         ARRAY(SELECT DISTINCT i.email FROM conbira_auth_user_identities i WHERE i.user_id = u.id ORDER BY i.email) AS emails,
         EXISTS (SELECT 1 FROM conbira_rbac_user_roles link WHERE link.user_id = u.id AND link.role_id = ");
    query
        .push_bind(role_id)
        .push(") AS assigned FROM conbira_auth_users u WHERE TRUE");
    if let Some(name) = criteria.name {
        query
            .push(" AND strpos(lower(u.nickname), lower(")
            .push_bind(name)
            .push(")) > 0");
    }
    if let Some(email) = criteria.email {
        query.push(" AND EXISTS (SELECT 1 FROM conbira_auth_user_identities i WHERE i.user_id = u.id AND strpos(lower(i.email), lower(").push_bind(email).push(")) > 0)");
    }
    if let Some(status) = criteria.status {
        query.push(" AND u.status = ").push_bind(status);
    }
    query
        .push(" ORDER BY u.id LIMIT ")
        .push_bind(criteria.limit)
        .push(" OFFSET ")
        .push_bind(criteria.offset);
    query
        .build_query_as::<RoleCandidate>()
        .fetch_all(handle.conn())
        .await
        .map_err_internal("Failed to search role candidates", crate::here!())
}

pub async fn delete(handle: &mut Handle<'_>, user_id: i64, role_id: i64) -> Result<bool> {
    let result =
        sqlx::query("DELETE FROM conbira_rbac_user_roles WHERE user_id = $1 AND role_id = $2")
            .bind(user_id)
            .bind(role_id)
            .execute(handle.conn())
            .await
            .map_err_internal("Failed to delete user_role", crate::here!())?;
    Ok(result.rows_affected() > 0)
}
