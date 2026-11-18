use crate::Result;
use crate::app::auth::entity::Role;
use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

pub async fn insert(handle: &mut Handle<'_>, user_id: i64, role_id: i64) -> Result<()> {
    sqlx::query("INSERT INTO conbira_user_roles (user_id, role_id) VALUES ($1, $2)")
        .bind(user_id)
        .bind(role_id)
        .execute(handle.conn())
        .await
        .map_err_internal("Failed to insert user_role", crate::here!())?;
    Ok(())
}

pub async fn list_by_user_id(handle: &mut Handle<'_>, user_id: i64) -> Result<Vec<Role>> {
    sqlx::query_as::<_, Role>(
        "SELECT e.id, e.code, e.name, e.description FROM conbira_roles e
         JOIN conbira_user_roles link ON link.role_id = e.id
         WHERE link.user_id = $1 ORDER BY e.id",
    )
    .bind(user_id)
    .fetch_all(handle.conn())
    .await
    .map_err_internal("Failed to list user_roles", crate::here!())
}

pub async fn delete(handle: &mut Handle<'_>, user_id: i64, role_id: i64) -> Result<bool> {
    let result = sqlx::query("DELETE FROM conbira_user_roles WHERE user_id = $1 AND role_id = $2")
        .bind(user_id)
        .bind(role_id)
        .execute(handle.conn())
        .await
        .map_err_internal("Failed to delete user_role", crate::here!())?;
    Ok(result.rows_affected() > 0)
}
