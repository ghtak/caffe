use crate::Result;
use crate::app::auth::entity::Permission;
use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

pub struct NewPermission {
    pub code: String,
    pub name: String,
    pub description: String,
}

pub async fn insert(handle: &mut Handle<'_>, value: &NewPermission) -> Result<Permission> {
    sqlx::query_as::<_, Permission>(
        "INSERT INTO conbira_rbac_permissions (code, name, description) VALUES ($1, $2, $3)
         RETURNING id, code, name, description",
    )
    .bind(&value.code)
    .bind(&value.name)
    .bind(&value.description)
    .fetch_one(handle.conn())
    .await
    .map_err_internal("Failed to insert permission", crate::here!())
}

pub async fn find_by_id(handle: &mut Handle<'_>, id: i64) -> Result<Option<Permission>> {
    sqlx::query_as::<_, Permission>(
        "SELECT id, code, name, description FROM conbira_rbac_permissions WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(handle.conn())
    .await
    .map_err_internal("Failed to find permission by id", crate::here!())
}

pub async fn find_by_code(handle: &mut Handle<'_>, code: &str) -> Result<Option<Permission>> {
    sqlx::query_as::<_, Permission>(
        "SELECT id, code, name, description FROM conbira_rbac_permissions WHERE code = $1",
    )
    .bind(code)
    .fetch_optional(handle.conn())
    .await
    .map_err_internal("Failed to find permission by code", crate::here!())
}

pub async fn list(handle: &mut Handle<'_>) -> Result<Vec<Permission>> {
    sqlx::query_as::<_, Permission>(
        "SELECT id, code, name, description FROM conbira_rbac_permissions ORDER BY id",
    )
    .fetch_all(handle.conn())
    .await
    .map_err_internal("Failed to list permissions", crate::here!())
}

pub async fn find_by_user_id(handle: &mut Handle<'_>, user_id: i64) -> Result<Vec<Permission>> {
    sqlx::query_as::<_, Permission>(
        "SELECT DISTINCT p.id, p.code, p.name, p.description
         FROM conbira_rbac_permissions p
         JOIN conbira_rbac_role_permissions rp ON rp.permission_id = p.id
         JOIN conbira_rbac_user_roles ur ON ur.role_id = rp.role_id
         WHERE ur.user_id = $1 ORDER BY p.id",
    )
    .bind(user_id)
    .fetch_all(handle.conn())
    .await
    .map_err_internal("Failed to find permissions by user", crate::here!())
}

pub async fn update(handle: &mut Handle<'_>, value: &Permission) -> Result<Option<Permission>> {
    sqlx::query_as::<_, Permission>(
        "UPDATE conbira_rbac_permissions SET code = $2, name = $3, description = $4
         WHERE id = $1 RETURNING id, code, name, description",
    )
    .bind(value.id)
    .bind(&value.code)
    .bind(&value.name)
    .bind(&value.description)
    .fetch_optional(handle.conn())
    .await
    .map_err_internal("Failed to update permission", crate::here!())
}

pub async fn delete(handle: &mut Handle<'_>, id: i64) -> Result<bool> {
    let result = sqlx::query("DELETE FROM conbira_rbac_permissions WHERE id = $1")
        .bind(id)
        .execute(handle.conn())
        .await
        .map_err_internal("Failed to delete permission", crate::here!())?;
    Ok(result.rows_affected() > 0)
}
