pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use uuid::Uuid;

    use crate::app::auth::claims::UserClaims;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::{ApiResponse, ApiResult};
    use crate::app::{Container, DatabaseExt as _};

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiPath(id): ApiPath<Uuid>,
    ) -> ApiResult<()> {
        caller.require_permission("file_asset.manage", crate::here!())?;
        let mut conn = container.db.acquire_conn(crate::here!()).await?;
        container
            .file_asset_service
            .delete(&mut conn.handle(), id)
            .await?;
        Ok(ApiResponse::empty())
    }
}
