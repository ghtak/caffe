pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::auth::claims::UserClaims;
    use crate::app::board::entity::Comment;
    use crate::app::board::repository::comment;
    use crate::app::http::extract::{ApiJson, ApiPath};
    use crate::app::http::response::ApiResult;
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct RequestBody {
        pub content: String,
    }

    impl espr::validate::Validate for RequestBody {
        type Error = crate::Error;
        fn validate(&self) -> crate::Result<()> {
            if self.content.trim().is_empty() {
                return Err(
                    Error::new(Code::InvalidRequest, "Comment content is required")
                        .at(crate::here!()),
                );
            }
            Ok(())
        }
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiPath(id): ApiPath<i64>,
        ApiJson(request): ApiJson<RequestBody>,
    ) -> ApiResult<Comment> {
        let mut conn = container.db.acquire_conn(crate::here!()).await?;
        let updated = comment::update_content(&mut conn.handle(), id, caller.id, &request.content)
            .await?
            .ok_or_else(|| Error::new(Code::NotFound, "Comment not found").at(crate::here!()))?;
        Ok(updated.into())
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
        async fn author_updates_content_and_other_users_cannot() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../../migrations/0020_posts.sql")).execute(conn.handle().conn()).await?;
                sqlx::raw_sql("INSERT INTO conbira_auth_users (nickname, status) VALUES ('author', 'active'), ('other', 'active'); INSERT INTO conbira_posts (board_slug, author_id, title, comment_count) VALUES ('free', 1, 'post', 1); INSERT INTO conbira_post_comments (post_id, author_id, content) VALUES (1, 1, 'old');").execute(conn.handle().conn()).await?;
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let router = crate::app::http::router(container.clone())?;
                for (user, status, content) in [(2, StatusCode::NOT_FOUND, "old"), (1, StatusCode::OK, "new")] {
                    let token = container.jwt_token_service.issue(&user.to_string(), UserClaims::new(user, UserStatus::Active, vec![]).try_into()?)?.token;
                    let response = router.clone().oneshot(Request::put("/api/board/comments/1").header("Authorization", format!("Bearer {token}")).header("Content-Type", "application/json").body(Body::from(r#"{"content":"new"}"#))?).await?;
                    assert_eq!(response.status(), status);
                    if status == StatusCode::OK {
                        let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                        assert_eq!(body["data"]["content"], "new");
                    }
                    let mut conn = context.db.conn().await?;
                    let stored: (String, i32) = sqlx::query_as("SELECT c.content, p.comment_count FROM conbira_post_comments c JOIN conbira_posts p ON p.id = c.post_id WHERE c.id = 1").fetch_one(conn.handle().conn()).await?;
                    assert_eq!(stored, (content.into(), 1));
                }
                Ok(())
            }).await
        }

        #[tokio::test]
        async fn rejects_unauthenticated_and_invalid_or_deleted_comments() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../../migrations/0020_posts.sql")).execute(conn.handle().conn()).await?;
                sqlx::raw_sql("INSERT INTO conbira_auth_users (nickname, status) VALUES ('author', 'active'); INSERT INTO conbira_posts (board_slug, author_id, title) VALUES ('free', 1, 'post'), ('free', 1, 'deleted'); UPDATE conbira_posts SET deleted_at = CURRENT_TIMESTAMP WHERE id = 2; INSERT INTO conbira_post_comments (post_id, author_id, content, deleted_at) VALUES (1, 1, 'active', NULL), (1, 1, 'deleted', CURRENT_TIMESTAMP), (2, 1, 'hidden', NULL);").execute(conn.handle().conn()).await?;
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let token = container.jwt_token_service.issue("1", UserClaims::new(1, UserStatus::Active, vec![]).try_into()?)?.token;
                let router = crate::app::http::router(container)?;
                for (id, body, authenticated, expected) in [(1, r#"{"content":"x"}"#, false, StatusCode::UNAUTHORIZED), (1, r#"{"content":" "}"#, true, StatusCode::BAD_REQUEST), (2, r#"{"content":"x"}"#, true, StatusCode::NOT_FOUND), (3, r#"{"content":"x"}"#, true, StatusCode::NOT_FOUND), (999, r#"{"content":"x"}"#, true, StatusCode::NOT_FOUND)] {
                    let mut request = Request::put(format!("/api/board/comments/{id}")).header("Content-Type", "application/json");
                    if authenticated { request = request.header("Authorization", format!("Bearer {token}")); }
                    let response = router.clone().oneshot(request.body(Body::from(body))?).await?;
                    assert_eq!(response.status(), expected);
                }
                Ok(())
            }).await
        }
    }
}
