pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use serde::Deserialize;

    use crate::app::auth::claims::UserClaims;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::{ApiResponse, ApiResult};
    use crate::app::rbac::repository::user_role;
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    #[derive(Deserialize)]
    pub struct RequestPath {
        user_id: i64,
        role_id: i64,
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        claims: UserClaims,
        ApiPath(RequestPath { user_id, role_id }): ApiPath<RequestPath>,
    ) -> ApiResult<()> {
        claims.require_permission("rbac.user_role.manage", crate::here!())?;
        let mut connection = container.db.acquire_conn(crate::here!()).await?;
        if !user_role::delete(&mut connection.handle(), user_id, role_id).await? {
            return Err(Error::new(Code::NotFound, "User role not found")
                .at(crate::here!())
                .into());
        }
        Ok(ApiResponse::empty())
    }
}
