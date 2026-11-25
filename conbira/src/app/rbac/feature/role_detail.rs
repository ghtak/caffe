pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::auth::claims::UserClaims;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::ApiResult;
    use crate::app::rbac::entity::Role;
    use crate::app::rbac::repository::role;
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    pub async fn handler(
        State(container): State<Arc<Container>>,
        claims: UserClaims,
        ApiPath(role_id): ApiPath<i64>,
    ) -> ApiResult<Role> {
        claims.require_permission("rbac.read", crate::here!())?;
        let mut connection = container.db.acquire_conn(crate::here!()).await?;
        let role = role::find_by_id(&mut connection.handle(), role_id)
            .await?
            .ok_or_else(|| Error::new(Code::NotFound, "Role not found").at(crate::here!()))?;
        Ok(role.into())
    }
}
