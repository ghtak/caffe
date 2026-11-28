pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::auth::claims::UserClaims;
    use crate::app::board::entity::Comment;
    use crate::app::http::extract::{ApiJson, ApiPath};
    use crate::app::http::response::ApiResult;

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct RequestBody {
        pub content: String,
        pub parent_id: Option<i64>,
    }

    impl espr::validate::Validate for RequestBody {
        type Error = crate::Error;
        fn validate(&self) -> crate::Result<()> {
            if self.content.trim().is_empty() {
                return Err(crate::Error::new(
                    crate::Code::InvalidRequest,
                    "Comment content is required",
                )
                .at(crate::here!()));
            }
            Ok(())
        }
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiPath(post_id): ApiPath<i64>,
        ApiJson(request): ApiJson<RequestBody>,
    ) -> ApiResult<Comment> {
        Ok(super::usecase::execute(
            &container.db,
            post_id,
            request.parent_id,
            caller.id,
            &request.content,
        )
        .await?
        .into())
    }
}

pub mod usecase {
    use crate::app::DatabaseExt as _;
    use crate::app::board::entity::Comment;
    use crate::app::board::repository::{comment, post};
    use crate::app::container::db::Database;
    use crate::error::ResultExt as _;
    use crate::{Code, Error};

    pub async fn execute(
        db: &Database,
        post_id: i64,
        parent_id: Option<i64>,
        author_id: i64,
        content: &str,
    ) -> crate::Result<Comment> {
        let mut tx = db.begin_tx(crate::here!()).await?;
        if !post::increment_comment_count(&mut tx.handle(), post_id).await? {
            return Err(Error::new(Code::NotFound, "Post not found").at(crate::here!()));
        }
        if let Some(parent_id) = parent_id
            && !comment::is_active_root(&mut tx.handle(), post_id, parent_id).await?
        {
            return Err(
                Error::new(Code::InvalidRequest, "Invalid parent comment").at(crate::here!())
            );
        }
        let created =
            comment::insert(&mut tx.handle(), post_id, parent_id, author_id, content).await?;
        tx.commit()
            .await
            .map_err_internal("Failed to commit comment creation", crate::here!())?;
        Ok(created)
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
        async fn creates_comments_and_rejects_invalid_parents_and_deleted_posts()
        -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../../migrations/0020_posts.sql")).execute(conn.handle().conn()).await?;
                sqlx::raw_sql("INSERT INTO conbira_auth_users (nickname, status) VALUES ('author', 'active'); INSERT INTO conbira_posts (board_slug, author_id, title) VALUES ('free', 1, 'first'), ('free', 1, 'second'), ('free', 1, 'deleted'); UPDATE conbira_posts SET deleted_at = CURRENT_TIMESTAMP WHERE id = 3;").execute(conn.handle().conn()).await?;
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let token = container.jwt_token_service.issue("1", UserClaims::new(1, UserStatus::Active, vec![]).try_into()?)?.token;
                let router = crate::app::http::router(container)?;
                for (post_id, body, expected) in [
                    (1, r#"{"content":"root"}"#, StatusCode::OK),
                    (1, r#"{"content":"reply","parent_id":1}"#, StatusCode::OK),
                    (1, r#"{"content":"nested","parent_id":2}"#, StatusCode::BAD_REQUEST),
                    (2, r#"{"content":"wrong post","parent_id":1}"#, StatusCode::BAD_REQUEST),
                    (3, r#"{"content":"deleted"}"#, StatusCode::NOT_FOUND),
                    (99, r#"{"content":"missing"}"#, StatusCode::NOT_FOUND),
                    (1, r#"{"content":" "}"#, StatusCode::BAD_REQUEST),
                    (1, r#"{"content":"secret","is_secret":true}"#, StatusCode::BAD_REQUEST),
                ] {
                    let response = router.clone().oneshot(Request::post(format!("/api/board/posts/{post_id}/comments")).header("Authorization", format!("Bearer {token}")).header("Content-Type", "application/json").body(Body::from(body))?).await?;
                    let status = response.status();
                    let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                    assert_eq!(status, expected, "{body}");
                    if status == StatusCode::OK { assert_eq!(body["data"]["author_id"], 1); }
                }
                let mut conn = context.db.conn().await?;
                let counts: Vec<(i64, i32)> = sqlx::query_as("SELECT id, comment_count FROM conbira_posts ORDER BY id").fetch_all(conn.handle().conn()).await?;
                assert_eq!(counts, vec![(1, 2), (2, 0), (3, 0)]);
                Ok(())
            }).await
        }

        #[tokio::test]
        async fn rejects_unauthenticated_comment() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(1, |context| async move {
                let router =
                    crate::app::http::router(std::sync::Arc::new(context.container().await?))?;
                let response = router
                    .oneshot(
                        Request::post("/api/board/posts/1/comments")
                            .header("Content-Type", "application/json")
                            .body(Body::from(r#"{"content":"hello"}"#))?,
                    )
                    .await?;
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
                Ok(())
            })
            .await
        }
    }
}
