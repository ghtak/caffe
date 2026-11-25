pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::auth::claims::UserClaims;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::ApiResult;
    use crate::app::rbac::entity::Permission;
    use crate::app::rbac::repository::{role, role_permission};
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    pub async fn handler(
        State(container): State<Arc<Container>>,
        claims: UserClaims,
        ApiPath(role_id): ApiPath<i64>,
    ) -> ApiResult<Vec<Permission>> {
        claims.require_permission("rbac.read", crate::here!())?;
        let mut connection = container.db.acquire_conn(crate::here!()).await?;
        let mut handle = connection.handle();
        if role::find_by_id(&mut handle, role_id).await?.is_none() {
            return Err(Error::new(Code::NotFound, "Role not found")
                .at(crate::here!())
                .into());
        }
        Ok(role_permission::list_by_role_id(&mut handle, role_id)
            .await?
            .into())
    }
}
