pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::board::entity::CommentSummary;
    use crate::app::board::repository::comment;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::ApiResult;
    use crate::app::{Container, DatabaseExt as _};

    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiPath(post_id): ApiPath<i64>,
    ) -> ApiResult<Vec<CommentSummary>> {
        let mut conn = container.db.acquire_conn(crate::here!()).await?;
        Ok(comment::list(&mut conn.handle(), post_id).await?.into())
    }
}

#[cfg(test)]
mod tests {
    mod contract {
        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt as _;

        #[tokio::test]
        async fn lists_public_comments_in_thread_order_and_excludes_deleted_and_secret()
        -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../../migrations/0020_posts.sql")).execute(conn.handle().conn()).await?;
                sqlx::raw_sql("INSERT INTO conbira_auth_users (nickname, status) VALUES ('writer', 'active'); INSERT INTO conbira_posts (board_slug, author_id, title) VALUES ('free', 1, 'post'), ('free', 1, 'other'), ('free', 1, 'deleted'); UPDATE conbira_posts SET deleted_at = CURRENT_TIMESTAMP WHERE id = 3; INSERT INTO conbira_post_comments (post_id, author_id, content, parent_id, is_secret, deleted_at) VALUES (1, 1, 'root', NULL, false, NULL), (1, 1, 'second root', NULL, false, NULL), (1, 1, 'reply', 1, false, NULL), (1, 1, 'secret', NULL, true, NULL), (1, 1, 'deleted', NULL, false, CURRENT_TIMESTAMP), (2, 1, 'other post', NULL, false, NULL), (1, 1, 'secret reply', 4, false, NULL);").execute(conn.handle().conn()).await?;
                sqlx::query("INSERT INTO conbira_post_comments (post_id, author_id, content) VALUES (3, 1, 'hidden')").execute(conn.handle().conn()).await?;
                drop(conn);
                let router = crate::app::http::router(std::sync::Arc::new(context.container().await?))?;
                let response = router.clone().oneshot(Request::get("/api/board/posts/1/comments").body(Body::empty())?).await?;
                assert_eq!(response.status(), StatusCode::OK);
                let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                let comments = body["data"].as_array().unwrap();
                assert_eq!(comments.iter().map(|c| c["content"].as_str().unwrap()).collect::<Vec<_>>(), vec!["root", "reply", "second root"]);
                assert_eq!(comments[0]["author_nickname"], "writer");
                for id in [3, 999] {
                    let response = router.clone().oneshot(Request::get(format!("/api/board/posts/{id}/comments")).body(Body::empty())?).await?;
                    assert_eq!(response.status(), StatusCode::OK);
                    let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                    assert_eq!(body["data"], serde_json::json!([]));
                }
                Ok(())
            }).await
        }
    }
}
