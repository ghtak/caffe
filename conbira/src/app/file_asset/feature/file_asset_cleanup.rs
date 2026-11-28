pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use axum::http::StatusCode;

    use crate::app::Container;
    use crate::app::auth::claims::UserClaims;
    use crate::app::http::response::{ApiResponse, ApiResult};

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
    ) -> ApiResult<()> {
        caller.require_permission("file_asset.manage", crate::here!())?;
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            runtime.block_on(async move {
                if let Err(error) = container.file_asset_service.cleanup(&container.db).await {
                    tracing::error!(%error, "File asset cleanup task failed");
                }
            });
        });
        Ok(ApiResponse::empty().with_status(StatusCode::ACCEPTED))
    }
}
