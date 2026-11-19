use crate::Result;
use crate::app::container::db::Handle;
use crate::app::rbac::entity::Role;
use crate::error::ResultExt as _;

pub struct NewRole {
    pub code: String,
    pub name: String,
    pub description: String,
}

pub async fn insert(handle: &mut Handle<'_>, value: &NewRole) -> Result<Role> {
    sqlx::query_as::<_, Role>(
        "INSERT INTO conbira_rbac_roles (code, name, description) VALUES ($1, $2, $3)
         ON CONFLICT (code) DO NOTHING
         RETURNING id, code, name, description",
    )
    .bind(&value.code)
    .bind(&value.name)
    .bind(&value.description)
    .fetch_optional(handle.conn())
    .await
    .map_err_internal("Failed to insert role", crate::here!())?
    .ok_or_else(|| {
        crate::Error::new(crate::Code::InvalidRequest, "Role code already exists")
            .at(crate::here!())
    })
}

pub async fn find_by_id(handle: &mut Handle<'_>, id: i64) -> Result<Option<Role>> {
    sqlx::query_as::<_, Role>(
        "SELECT id, code, name, description FROM conbira_rbac_roles WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(handle.conn())
    .await
    .map_err_internal("Failed to find role by id", crate::here!())
}

pub async fn find_by_code(handle: &mut Handle<'_>, code: &str) -> Result<Option<Role>> {
    sqlx::query_as::<_, Role>(
        "SELECT id, code, name, description FROM conbira_rbac_roles WHERE code = $1",
    )
    .bind(code)
    .fetch_optional(handle.conn())
    .await
    .map_err_internal("Failed to find role by code", crate::here!())
}

pub async fn list(handle: &mut Handle<'_>) -> Result<Vec<Role>> {
    sqlx::query_as::<_, Role>(
        "SELECT id, code, name, description FROM conbira_rbac_roles ORDER BY id",
    )
    .fetch_all(handle.conn())
    .await
    .map_err_internal("Failed to list roles", crate::here!())
}

pub async fn update(handle: &mut Handle<'_>, value: &Role) -> Result<Option<Role>> {
    sqlx::query_as::<_, Role>(
        "UPDATE conbira_rbac_roles SET code = $2, name = $3, description = $4
         WHERE id = $1 RETURNING id, code, name, description",
    )
    .bind(value.id)
    .bind(&value.code)
    .bind(&value.name)
    .bind(&value.description)
    .fetch_optional(handle.conn())
    .await
    .map_err_internal("Failed to update role", crate::here!())
}

pub async fn delete(handle: &mut Handle<'_>, id: i64) -> Result<bool> {
    let result = sqlx::query("DELETE FROM conbira_rbac_roles WHERE id = $1")
        .bind(id)
        .execute(handle.conn())
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .is_some_and(|e| e.is_foreign_key_violation())
            {
                crate::Error::new(
                    crate::Code::InvalidRequest,
                    "Role is assigned to a user or permission",
                )
                .at(crate::here!())
                .cause(error)
            } else {
                crate::Error::internal("Failed to delete role")
                    .at(crate::here!())
                    .cause(error)
            }
        })?;
    Ok(result.rows_affected() > 0)
}
