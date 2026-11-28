pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::auth::claims::UserClaims;
    use crate::app::board::entity::Post;
    use crate::app::http::extract::{ApiJson, ApiPath};
    use crate::app::http::response::ApiResult;

    #[derive(serde::Deserialize)]
    pub struct RequestBody {
        pub title: String,
        pub content: String,
        pub attachments: Option<crate::app::board::AttachmentChanges>,
    }

    impl espr::validate::Validate for RequestBody {
        type Error = crate::Error;

        fn validate(&self) -> crate::Result<()> {
            if self.title.trim().is_empty()
                || self.title.trim().chars().count() > 255
                || self.content.trim().is_empty()
            {
                return Err(crate::Error::new(
                    crate::Code::InvalidRequest,
                    "Invalid title or content",
                )
                .at(crate::here!()));
            }
            Ok(())
        }
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiPath(id): ApiPath<i64>,
        ApiJson(request): ApiJson<RequestBody>,
    ) -> ApiResult<Post> {
        Ok(super::usecase::execute(
            &container.db,
            id,
            caller,
            &container.file_asset_service,
            request.attachments,
            request.title.trim(),
            &request.content,
        )
        .await?
        .into())
    }
}

#[cfg(test)]
mod tests {
    mod contract {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt as _;

        use crate::app::auth::claims::UserClaims;
        use crate::app::auth::entity::UserStatus;

        #[tokio::test]
        async fn author_can_update_title_and_content_but_other_users_cannot() -> anyhow::Result<()>
        {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../../migrations/0020_posts.sql")).execute(conn.handle().conn()).await?;
                sqlx::raw_sql("INSERT INTO conbira_auth_users (nickname, status) VALUES ('author', 'active'), ('other', 'active'); INSERT INTO conbira_posts (board_slug, author_id, title) VALUES ('data', 1, 'old'); INSERT INTO conbira_post_contents (post_id, content) VALUES (1, 'old body');").execute(conn.handle().conn()).await?;
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let router = crate::app::http::router(container.clone())?;
                for (user, expected) in [(2, StatusCode::FORBIDDEN), (1, StatusCode::OK)] {
                    let token = container.jwt_token_service.issue(&user.to_string(), UserClaims::new(user, UserStatus::Active, vec![]).try_into()?)?.token;
                    let response = router.clone().oneshot(Request::put("/api/board/posts/1").header("Authorization", format!("Bearer {token}")).header("Content-Type", "application/json").body(Body::from(r#"{"title":"new","content":"new body"}"#))?).await?;
                    assert_eq!(response.status(), expected);
                    let mut conn = context.db.conn().await?;
                    let stored: (String, String) = sqlx::query_as("SELECT p.title, c.content FROM conbira_posts p JOIN conbira_post_contents c ON c.post_id = p.id WHERE p.id = 1").fetch_one(conn.handle().conn()).await?;
                    let expected_values = if user == 1 { ("new", "new body") } else { ("old", "old body") };
                    assert_eq!((stored.0.as_str(), stored.1.as_str()), expected_values);
                }
                Ok(())
            }).await
        }

        #[test]
        fn rejects_blank_title_or_content_and_long_title() {
            use espr::validate::Validate as _;
            for (title, content) in [
                (" ".into(), "body"),
                ("title".into(), " "),
                ("가".repeat(256), "body"),
            ] {
                assert!(
                    super::super::endpoint::RequestBody {
                        title,
                        content: content.into(),
                        attachments: None
                    }
                    .validate()
                    .is_err()
                );
            }
        }

        #[tokio::test]
        async fn rejects_unauthenticated_update() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(1, |context| async move {
                let router =
                    crate::app::http::router(std::sync::Arc::new(context.container().await?))?;
                let response = router
                    .oneshot(
                        Request::put("/api/board/posts/1")
                            .header("Content-Type", "application/json")
                            .body(Body::from(r#"{"title":"new","content":"new body"}"#))?,
                    )
                    .await?;
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
                Ok(())
            })
            .await
        }
    }
}

pub mod usecase {
    use crate::app::DatabaseExt as _;
    use crate::app::board::entity::Post;
    use crate::app::board::repository::{post, post_content};
    use crate::app::container::db::Database;
    use crate::error::ResultExt as _;
    use crate::{Code, Error};

    pub async fn execute(
        db: &Database,
        id: i64,
        caller: crate::app::auth::claims::UserClaims,
        service: &crate::app::file_asset::service::FileAssetService,
        attachments: Option<crate::app::board::AttachmentChanges>,
        title: &str,
        content: &str,
    ) -> crate::Result<Post> {
        let mut tx = db.begin_tx(crate::here!()).await?;
        let existing = post::find_by_id(&mut tx.handle(), id)
            .await?
            .ok_or_else(|| Error::new(Code::NotFound, "Post not found").at(crate::here!()))?;
        if existing.summary.post.author_id != caller.id {
            return Err(Error::new(Code::Forbidden, "Post access denied").at(crate::here!()));
        }
        let updated = post::update_title(&mut tx.handle(), id, caller.id, title)
            .await?
            .ok_or_else(|| Error::new(Code::NotFound, "Post not found").at(crate::here!()))?;
        post_content::update(&mut tx.handle(), id, content).await?;
        if let Some(changes) = attachments {
            super::super::super::change_attachments(
                &mut tx.handle(),
                service,
                caller.id,
                &updated,
                changes,
            )
            .await?;
        }
        tx.commit()
            .await
            .map_err_internal("Failed to commit post update", crate::here!())?;
        Ok(updated)
    }
}
