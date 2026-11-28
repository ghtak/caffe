pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::auth::claims::UserClaims;
    use crate::app::file_asset::service::{UploadRequest, UploadTicket};
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::ApiResult;
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    #[derive(serde::Deserialize)]
    pub struct RequestBody {
        pub board_slug: String,
        pub original_filename: String,
        pub size_bytes: i64,
        pub content_type: String,
    }

    impl espr::validate::Validate for RequestBody {
        type Error = Error;
        fn validate(&self) -> crate::Result<()> {
            if self.board_slug.trim().is_empty() || self.board_slug.trim().chars().count() > 64 {
                return Err(
                    Error::new(Code::InvalidRequest, "Invalid board slug").at(crate::here!())
                );
            }
            Ok(())
        }
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiJson(body): ApiJson<RequestBody>,
    ) -> ApiResult<UploadTicket> {
        caller.require_permission(
            &format!("board.{}.write", body.board_slug.trim()),
            crate::here!(),
        )?;
        let mut conn = container.db.acquire_conn(crate::here!()).await?;
        Ok(container
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
            .await?
            .into())
    }
}

#[cfg(test)]
mod tests {
    mod contract {
        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt as _;

        use crate::app::auth::claims::UserClaims;
        use crate::app::auth::entity::UserStatus;

        #[tokio::test]
        async fn upload_requires_matching_board_write_permission_and_records_caller()
        -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('writer') RETURNING id").fetch_one(conn.handle().conn()).await?;
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let issue = |permissions: Vec<String>| -> anyhow::Result<String> { Ok(container.jwt_token_service.issue(&user.to_string(), UserClaims::new(user, UserStatus::Active, permissions).try_into()?)?.token) };
                let writer = issue(vec!["board.data.write".into()])?;
                let other = issue(vec!["board.free.write".into(), "file_asset.manage".into()])?;
                let router = crate::app::http::router(container)?;
                let body = serde_json::json!({"board_slug":"data", "original_filename":"report.txt", "size_bytes":4, "content_type":"text/plain", "uploaded_by":999}).to_string();
                let request = |token: &str| Request::post("/api/board/posts/attachments/upload").header("Authorization", format!("Bearer {token}")).header("content-type", "application/json").body(Body::from(body.clone()));
                assert_eq!(router.clone().oneshot(request(&other)?).await?.status(), StatusCode::FORBIDDEN);
                let response = router.oneshot(request(&writer)?).await?;
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(response.headers()["cache-control"], "no-store");
                let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                assert!(body["data"]["upload_url"].as_str().unwrap().contains("X-Amz-Signature="));
                let id = uuid::Uuid::parse_str(body["data"]["id"].as_str().unwrap())?;
                let mut conn = context.db.conn().await?;
                let row: (i64, String) = sqlx::query_as("SELECT uploaded_by, status FROM conbira_file_assets WHERE id=$1").bind(id).fetch_one(conn.handle().conn()).await?;
                assert_eq!(row, (user, "pending".into()));
                Ok(())
            }).await
        }
    }
}
