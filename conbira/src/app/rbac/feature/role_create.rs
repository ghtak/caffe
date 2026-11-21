pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use espr_derive::Validate;
    use serde::Deserialize;

    use crate::app::auth::claims::UserClaims;
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::ApiResult;
    use crate::app::rbac::entity::Role;
    use crate::app::rbac::repository::role::{self, NewRole};
    use crate::app::{Container, DatabaseExt as _};

    #[derive(Debug, Deserialize, Validate)]
    pub struct RequestBody {
        pub code: String,
        pub name: String,
        pub description: String,
    }

    pub type ResponseData = Role;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        claims: UserClaims,
        ApiJson(request): ApiJson<RequestBody>,
    ) -> ApiResult<ResponseData> {
        claims.require_permission("rbac.role.manage", crate::here!())?;
        let mut connection = container.db.acquire_conn(crate::here!()).await?;
        let value = NewRole {
            code: request.code,
            name: request.name,
            description: request.description,
        };
        Ok((role::insert(&mut connection.handle(), &value).await?).into())
    }
}
