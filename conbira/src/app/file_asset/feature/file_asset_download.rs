pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use uuid::Uuid;

    use crate::app::auth::claims::UserClaims;
    use crate::app::file_asset::service::DownloadTicket;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::ApiResult;
    use crate::app::{Container, DatabaseExt as _};

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiPath(id): ApiPath<Uuid>,
    ) -> ApiResult<DownloadTicket> {
        caller.require_permission("file_asset.list.read", crate::here!())?;
        let mut conn = container.db.acquire_conn(crate::here!()).await?;
        Ok(container
            .file_asset_service
            .prepare_download(&mut conn.handle(), id)
            .await?
            .into())
    }
}
