pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use espr_derive::Validate;
    use serde::Deserialize;

    use crate::app::auth::claims::UserClaims;
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::{ApiResponse, ApiResult};
    use crate::app::rbac::entity::Permission;
    use crate::app::rbac::repository::permission::{self, NewPermission};
    use crate::app::{Container, DatabaseExt as _};

    #[derive(Debug, Deserialize, Validate)]
    pub struct CreateRequest {
        pub code: String,
        pub name: String,
        pub description: String,
    }

    pub type CreateResponse = Permission;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        claims: UserClaims,
        ApiJson(request): ApiJson<CreateRequest>,
    ) -> ApiResult<CreateResponse> {
        claims.require_permission("rbac.permission.manage", crate::here!())?;
        let mut connection = container.db.acquire_conn(crate::here!()).await?;
        let value = NewPermission {
            code: request.code,
            name: request.name,
            description: request.description,
        };
        Ok(ApiResponse::new(
            permission::insert(&mut connection.handle(), &value).await?,
        ))
    }
}
