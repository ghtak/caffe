use crate::Result;
use crate::app::container::db::Handle;
use crate::app::rbac::entity::Permission;
use crate::error::ResultExt as _;

pub async fn insert(handle: &mut Handle<'_>, role_id: i64, permission_id: i64) -> Result<()> {
    sqlx::query(
        "INSERT INTO conbira_rbac_role_permissions (role_id, permission_id) VALUES ($1, $2)",
    )
    .bind(role_id)
    .bind(permission_id)
    .execute(handle.conn())
    .await
    .map_err(|error| {
        let code = error.as_database_error();
        let (kind, message) = if code.is_some_and(|e| e.is_unique_violation()) {
            (crate::Code::InvalidRequest, "Role already has permission")
        } else if code.is_some_and(|e| e.is_foreign_key_violation()) {
            (crate::Code::NotFound, "Role or permission not found")
        } else {
            return crate::Error::internal("Failed to insert role_permission")
                .at(crate::here!())
                .cause(error);
        };
        crate::Error::new(kind, message)
            .at(crate::here!())
            .cause(error)
    })?;
    Ok(())
}

pub async fn list_by_role_id(handle: &mut Handle<'_>, role_id: i64) -> Result<Vec<Permission>> {
    sqlx::query_as::<_, Permission>(
        "SELECT e.id, e.code, e.name, e.description FROM conbira_rbac_permissions e
         JOIN conbira_rbac_role_permissions link ON link.permission_id = e.id
         WHERE link.role_id = $1 ORDER BY e.id",
    )
    .bind(role_id)
    .fetch_all(handle.conn())
    .await
    .map_err_internal("Failed to list role_permissions", crate::here!())
}

pub async fn delete(handle: &mut Handle<'_>, role_id: i64, permission_id: i64) -> Result<bool> {
    let result = sqlx::query(
        "DELETE FROM conbira_rbac_role_permissions WHERE role_id = $1 AND permission_id = $2",
    )
    .bind(role_id)
    .bind(permission_id)
    .execute(handle.conn())
    .await
    .map_err_internal("Failed to delete role_permission", crate::here!())?;
    Ok(result.rows_affected() > 0)
}
