pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use uuid::Uuid;

    use crate::app::auth::claims::UserClaims;
    use crate::app::file_asset::service::FileAssetStatus;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::{ApiResponse, ApiResult};
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiPath(id): ApiPath<Uuid>,
    ) -> ApiResult<()> {
        caller.require_permission("file_asset.manage", crate::here!())?;
        let mut conn = container.db.acquire_conn(crate::here!()).await?;
        if !container
            .file_asset_service
            .change_status(
                &mut conn.handle(),
                id,
                FileAssetStatus::Pending,
                FileAssetStatus::Attached,
            )
            .await?
        {
            return Err(
                Error::new(Code::InvalidRequest, "File asset is missing or not pending")
                    .at(crate::here!())
                    .into(),
            );
        }
        Ok(ApiResponse::empty())
    }
}
