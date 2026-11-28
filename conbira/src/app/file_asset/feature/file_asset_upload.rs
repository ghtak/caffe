pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::auth::claims::UserClaims;
    use crate::app::file_asset::service::{UploadRequest, UploadTicket};
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::ApiResult;
    use crate::app::{Container, DatabaseExt as _};

    #[derive(serde::Deserialize, espr_derive::Validate)]
    pub struct RequestBody {
        pub original_filename: String,
        pub size_bytes: i64,
        pub content_type: String,
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiJson(body): ApiJson<RequestBody>,
    ) -> ApiResult<UploadTicket> {
        caller.require_permission("file_asset.manage", crate::here!())?;
        let mut conn = container.db.acquire_conn(crate::here!()).await?;
        let ticket = container
            .file_asset_service
            .prepare_upload(
                &mut conn.handle(),
                UploadRequest {
                    uploaded_by: caller.id,
                    original_filename: body.original_filename,
                    size_bytes: body.size_bytes,
                    content_type: body.content_type,
                },
            )
            .await?;
        Ok(ticket.into())
    }
}
