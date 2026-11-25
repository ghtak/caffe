pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::auth::feature::user_token_refresh::endpoint::RequestBody;
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::{ApiResponse, ApiResult};
    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiJson(request): ApiJson<RequestBody>,
    ) -> ApiResult<()> {
        super::usecase::execute(
            &container.db,
            &container.refresh_token_service,
            &request.refresh_token,
        )
        .await?;
        Ok(ApiResponse::empty())
    }
}
pub mod usecase {
    use crate::app::DatabaseExt as _;
    use crate::app::auth::service::refresh_token::RefreshTokenService;
    use crate::app::container::db::Database;
    use crate::error::ResultExt as _;
    pub async fn execute(
        db: &Database,
        service: &RefreshTokenService,
        token: &str,
    ) -> crate::Result<()> {
        let mut tx = db.begin_tx(crate::here!()).await?;
        service.revoke(&mut tx.handle(), token).await?;
        tx.commit()
            .await
            .map_err_internal("Failed to commit logout", crate::here!())?;
        Ok(())
    }
}
