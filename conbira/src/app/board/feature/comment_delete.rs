pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::auth::claims::UserClaims;
    use crate::app::board::repository::comment;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::{ApiResponse, ApiResult};
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiPath(id): ApiPath<i64>,
    ) -> ApiResult<()> {
        let mut conn = container.db.acquire_conn(crate::here!()).await?;
        if !comment::delete(&mut conn.handle(), id, caller.id).await? {
            return Err(Error::new(Code::NotFound, "Comment not found")
                .at(crate::here!())
                .into());
        }
        Ok(ApiResponse::empty())
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
        async fn author_deletes_comment_once_and_list_masks_content_preserving_reply()
        -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../../migrations/0020_posts.sql")).execute(conn.handle().conn()).await?;
                sqlx::raw_sql("INSERT INTO conbira_auth_users (nickname, status) VALUES ('author', 'active'), ('other', 'active'); INSERT INTO conbira_posts (board_slug, author_id, title, comment_count) VALUES ('free', 1, 'post', 2); INSERT INTO conbira_post_comments (post_id, author_id, content, parent_id) VALUES (1, 1, 'private old content', NULL), (1, 2, 'reply', 1)").execute(conn.handle().conn()).await?;
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let router = crate::app::http::router(container.clone())?;
                let token = |id: i64| -> anyhow::Result<String> { Ok(container.jwt_token_service.issue(&id.to_string(), UserClaims::new(id, UserStatus::Active, vec![]).try_into()?)?.token) };
                for (id, expected) in [(2, StatusCode::NOT_FOUND), (1, StatusCode::OK), (1, StatusCode::NOT_FOUND)] {
                    let response = router.clone().oneshot(Request::delete("/api/board/comments/1").header("Authorization", format!("Bearer {}", token(id)?)).body(Body::empty())?).await?;
                    assert_eq!(response.status(), expected);
                }
                let response = router.oneshot(Request::get("/api/board/posts/1/comments").body(Body::empty())?).await?;
                let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                assert_eq!(body["data"][0]["is_deleted"], true);
                assert_eq!(body["data"][0]["content"], "");
                assert_eq!(body["data"][1]["content"], "reply");
                let mut conn = context.db.conn().await?;
                let count: i32 = sqlx::query_scalar("SELECT comment_count FROM conbira_posts WHERE id = 1").fetch_one(conn.handle().conn()).await?;
                assert_eq!(count, 1);
                Ok(())
            }).await
        }
    }
}
