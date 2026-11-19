pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::auth::claims::UserClaims;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::{ApiResponse, ApiResult};
    use crate::app::rbac::repository::role_permission;
    use crate::app::{Container, DatabaseExt as _};

    pub async fn handler(
        State(container): State<Arc<Container>>,
        claims: UserClaims,
        ApiPath((role_id, permission_id)): ApiPath<(i64, i64)>,
    ) -> ApiResult<()> {
        claims.require_permission("rbac.role.manage", crate::here!())?;
        let mut connection = container.db.acquire_conn(crate::here!()).await?;
        role_permission::insert(&mut connection.handle(), role_id, permission_id).await?;
        Ok(ApiResponse::empty())
    }
}
