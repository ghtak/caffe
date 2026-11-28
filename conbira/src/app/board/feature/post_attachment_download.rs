pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use uuid::Uuid;

    use crate::app::auth::claims::UserClaims;
    use crate::app::board::repository::post_attachment;
    use crate::app::file_asset::service::DownloadTicket;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::ApiResult;
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    pub async fn handler(
        State(container): State<Arc<Container>>,
        _caller: UserClaims,
        ApiPath((post_id, asset_id)): ApiPath<(i64, Uuid)>,
    ) -> ApiResult<DownloadTicket> {
        let mut conn = container.db.acquire_conn(crate::here!()).await?;
        if !post_attachment::is_downloadable(&mut conn.handle(), post_id, asset_id).await? {
            return Err(Error::new(Code::NotFound, "Post attachment not found")
                .at(crate::here!())
                .into());
        }
        Ok(container
            .file_asset_service
            .prepare_download(&mut conn.handle(), asset_id)
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
        async fn download_requires_login_and_active_post_attachment() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../../migrations/0020_posts.sql")).execute(conn.handle().conn()).await?;
                let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('reader') RETURNING id").fetch_one(conn.handle().conn()).await?;
                let post: i64 = sqlx::query_scalar("INSERT INTO conbira_posts (board_slug, author_id, title) VALUES ('data', $1, 'post') RETURNING id").bind(user).fetch_one(conn.handle().conn()).await?;
                let deleted_post: i64 = sqlx::query_scalar("INSERT INTO conbira_posts (board_slug, author_id, title, deleted_at) VALUES ('data', $1, 'deleted', CURRENT_TIMESTAMP) RETURNING id").bind(user).fetch_one(conn.handle().conn()).await?;
                let attached = uuid::Uuid::new_v4();
                let pending = uuid::Uuid::new_v4();
                let deleted = uuid::Uuid::new_v4();
                let unlinked = uuid::Uuid::new_v4();
                for (id, status) in [(attached, "attached"), (pending, "pending"), (deleted, "deleted"), (unlinked, "attached")] {
                    sqlx::query("INSERT INTO conbira_file_assets (id, status, uploaded_by, s3_bucket, s3_object_key, original_filename, size_bytes, content_type) VALUES ($1, $2, $3, 'bucket', $1::text, 'file.txt', 4, 'text/plain')").bind(id).bind(status).bind(user).execute(conn.handle().conn()).await?;
                }
                for (post_id, asset_id) in [(post, attached), (post, pending), (post, deleted), (deleted_post, attached)] {
                    sqlx::query("INSERT INTO conbira_post_attachments (post_id, file_asset_id) VALUES ($1, $2)").bind(post_id).bind(asset_id).execute(conn.handle().conn()).await?;
                }
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let token = container.jwt_token_service.issue(&user.to_string(), UserClaims::new(user, UserStatus::Active, vec![]).try_into()?)?.token;
                let router = crate::app::http::router(container)?;
                let path = |post_id, id| format!("/api/board/posts/{post_id}/attachments/{id}");
                assert_eq!(router.clone().oneshot(Request::get(path(post, attached)).body(Body::empty())?).await?.status(), StatusCode::UNAUTHORIZED);
                let request = |post_id, id| Request::get(path(post_id, id)).header("Authorization", format!("Bearer {token}")).body(Body::empty());
                for (post_id, id) in [(post, pending), (post, deleted), (post, unlinked), (deleted_post, attached), (post + 999, attached)] {
                    assert_eq!(router.clone().oneshot(request(post_id, id)?).await?.status(), StatusCode::NOT_FOUND);
                }
                let response = router.oneshot(request(post, attached)?).await?;
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(response.headers()["cache-control"], "no-store");
                let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                assert!(body["data"]["download_url"].as_str().unwrap().contains("X-Amz-Signature="));
                Ok(())
            }).await
        }
    }
}
