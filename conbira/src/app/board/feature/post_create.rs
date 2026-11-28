pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::auth::claims::UserClaims;
    use crate::app::board::entity::Post;
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::ApiResult;

    #[derive(serde::Deserialize)]
    pub struct RequestBody {
        pub board_slug: String,
        pub title: String,
        pub content: String,
        pub attachments: Option<crate::app::board::AttachmentChanges>,
    }

    impl espr::validate::Validate for RequestBody {
        type Error = crate::Error;

        fn validate(&self) -> crate::Result<()> {
            if self.board_slug.trim().is_empty()
                || self.board_slug.trim().chars().count() > 64
                || self.title.trim().is_empty()
                || self.title.trim().chars().count() > 255
                || self.content.trim().is_empty()
            {
                return Err(crate::Error::new(
                    crate::Code::InvalidRequest,
                    "Invalid board slug, title or content",
                )
                .at(crate::here!()));
            }
            Ok(())
        }
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiJson(request): ApiJson<RequestBody>,
    ) -> ApiResult<Post> {
        caller.require_permission(
            &format!("board.{}.write", request.board_slug.trim()),
            crate::here!(),
        )?;
        Ok(super::usecase::execute(
            &container.db,
            container.config.files.pending_ttl_seconds,
            caller,
            super::usecase::Command {
                board_slug: request.board_slug.trim().to_owned(),
                title: request.title.trim().to_owned(),
                content: request.content,
                attachments: request.attachments,
            },
        )
        .await?
        .into())
    }
}

pub mod usecase {
    use crate::app::DatabaseExt as _;
    use crate::app::board::entity::Post;
    use crate::app::board::repository::{post, post_content};
    use crate::app::container::db::Database;
    use crate::error::ResultExt as _;

    pub struct Command {
        pub board_slug: String,
        pub title: String,
        pub content: String,
        pub attachments: Option<crate::app::board::AttachmentChanges>,
    }

    pub async fn execute(
        db: &Database,
        ttl: u64,
        caller: crate::app::auth::claims::UserClaims,
        command: Command,
    ) -> crate::Result<Post> {
        let mut tx = db.begin_tx(crate::here!()).await?;
        let mut post = post::insert(
            &mut tx.handle(),
            &post::NewPost {
                board_slug: command.board_slug,
                title: command.title,
                author_id: caller.id,
            },
        )
        .await?;
        post_content::insert(&mut tx.handle(), post.id, &command.content).await?;
        if let Some(changes) = command.attachments {
            super::super::super::change_attachments(
                &mut tx.handle(),
                caller,
                &mut post,
                changes,
                ttl,
            )
            .await?;
        }
        tx.commit()
            .await
            .map_err_internal("Failed to commit post creation", crate::here!())?;
        Ok(post)
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
        async fn creates_post_and_content_with_authenticated_author() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../../migrations/0020_posts.sql"))
                    .execute(conn.handle().conn()).await?;
                let author_id: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname, status) VALUES ('author', 'active') RETURNING id")
                    .fetch_one(conn.handle().conn()).await?;
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let router = crate::app::http::router(container.clone())?;
                for board_slug in ["free", "notice", "data"] {
                    let token = container.jwt_token_service.issue(&author_id.to_string(), UserClaims::new(author_id, UserStatus::Active, vec![format!("board.{board_slug}.write")]).try_into()?)?.token;
                    let response = router.clone().oneshot(Request::post("/api/board/posts")
                        .header("Authorization", format!("Bearer {token}"))
                        .header("Content-Type", "application/json")
                        .body(Body::from(serde_json::json!({"board_slug":board_slug, "title":"제목", "content":"본문\n내용"}).to_string()))?).await?;
                    let status = response.status();
                    let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                    // 해당 게시판 작성 권한이 있으면 글을 작성하고 작성자는 인증 ID로 저장된다.
                    assert_eq!(status, StatusCode::OK, "{body}");
                    assert_eq!(body["data"]["author_id"], author_id);
                    assert_eq!(body["data"]["title"], "제목");
                    let id = body["data"]["id"].as_i64().unwrap();
                    let mut conn = context.db.conn().await?;
                    let stored: (String, String, String) = sqlx::query_as("SELECT p.board_slug, c.content, c.content_format FROM conbira_posts p JOIN conbira_post_contents c ON c.post_id = p.id WHERE p.id = $1")
                        .bind(id).fetch_one(conn.handle().conn()).await?;
                    // 글과 본문을 함께 저장하고 기본 본문 형식을 사용한다.
                    assert_eq!(stored, (board_slug.into(), "본문\n내용".into(), "MARKDOWN".into()));
                }
                Ok(())
            }).await
        }

        #[tokio::test]
        async fn rejects_creation_without_matching_board_write_permission() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../../migrations/0020_posts.sql"))
                    .execute(conn.handle().conn()).await?;
                sqlx::query("INSERT INTO conbira_auth_users (nickname, status) VALUES ('author', 'active')")
                    .execute(conn.handle().conn()).await?;
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let router = crate::app::http::router(container.clone())?;
                for board_slug in ["free", "notice", "data"] {
                    let other_board = if board_slug == "free" { "notice" } else { "free" };
                    for permissions in [vec![], vec![format!("board.{other_board}.write")], vec![format!("board.{board_slug}.manage")]] {
                        let token = container.jwt_token_service.issue("1", UserClaims::new(1, UserStatus::Active, permissions).try_into()?)?.token;
                        let response = router.clone().oneshot(Request::post("/api/board/posts")
                            .header("Authorization", format!("Bearer {token}"))
                            .header("Content-Type", "application/json")
                            .body(Body::from(serde_json::json!({"board_slug":board_slug, "title":"title", "content":"body"}).to_string()))?).await?;
                        // 권한이 없거나 다른 게시판 작성 권한·해당 게시판 관리 권한만 있으면 거부한다.
                        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{board_slug}");
                    }
                }
                let mut conn = context.db.conn().await?;
                let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM conbira_posts), (SELECT count(*) FROM conbira_post_contents)")
                    .fetch_one(conn.handle().conn()).await?;
                // 거부한 요청은 게시글과 본문을 저장하지 않는다.
                assert_eq!(counts, (0, 0));
                Ok(())
            }).await
        }

        #[tokio::test]
        async fn rejects_unauthenticated_creation() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(1, |context| async move {
                let router =
                    crate::app::http::router(std::sync::Arc::new(context.container().await?))?;
                let response = router
                    .oneshot(
                        Request::post("/api/board/posts")
                            .header("Content-Type", "application/json")
                            .body(Body::from(
                                r#"{"board_slug":"free","title":"title","content":"body"}"#,
                            ))?,
                    )
                    .await?;
                // 인증하지 않은 요청은 저장 전에 거절한다.
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
                Ok(())
            })
            .await
        }

        #[test]
        fn rejects_blank_fields_and_database_length_overflow() {
            use espr::validate::Validate as _;
            for (board_slug, title, content) in [
                (" ".into(), "title".into(), "body"),
                ("free".into(), " ".into(), "body"),
                ("free".into(), "title".into(), " \n"),
                ("x".repeat(65), "title".into(), "body"),
                ("free".into(), "가".repeat(256), "body"),
            ] {
                let request = super::super::endpoint::RequestBody {
                    board_slug,
                    title,
                    content: content.into(),
                    attachments: None,
                };
                // 필수값과 DB 컬럼 길이 제한을 API 경계에서 검사한다.
                assert!(request.validate().is_err());
            }
            let request = super::super::endpoint::RequestBody {
                board_slug: "x".repeat(64),
                title: "가".repeat(255),
                content: "body".into(),
                attachments: None,
            };
            // 길이 상한과 한글 제목은 허용한다.
            assert!(request.validate().is_ok());
        }
    }

    mod edge {
        #[tokio::test]
        async fn rolls_back_post_when_content_insert_fails() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../../migrations/0020_posts.sql"))
                    .execute(conn.handle().conn()).await?;
                sqlx::raw_sql("INSERT INTO conbira_auth_users (nickname, status) VALUES ('author', 'active');
                    ALTER TABLE conbira_post_contents ADD CONSTRAINT reject_content CHECK (content <> 'rejected');")
                    .execute(conn.handle().conn()).await?;
                drop(conn);
                let result = super::super::usecase::execute(&context.db, 3600, crate::app::auth::claims::UserClaims::new(1, crate::app::auth::entity::UserStatus::Active, vec![]), super::super::usecase::Command {
                    board_slug: "free".into(), attachments: None, title: "title".into(), content: "rejected".into(),
                }).await;
                let mut conn = context.db.conn().await?;
                let count: i64 = sqlx::query_scalar("SELECT count(*) FROM conbira_posts")
                    .fetch_one(conn.handle().conn()).await?;
                // 본문 저장 실패 시 메타데이터도 롤백한다.
                assert!(result.is_err());
                assert_eq!(count, 0);
                Ok(())
            }).await
        }
    }
}

#[cfg(test)]
mod attachment_tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt as _;

    use crate::app::auth::claims::UserClaims;
    use crate::app::auth::entity::UserStatus;
    use crate::app::files::access::Action;
    use crate::app::files::entity::FileStatus;
    use crate::app::files::repository;

    #[tokio::test]
    async fn creates_attachment_atomically_and_enforces_post_access() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(3, |context| async move {
            let fixture = crate::test_support::files::FileFixture::new(context.clone()).await?;
            let mut conn = context.db.conn().await?;
            sqlx::raw_sql(include_str!("../../../../migrations/0020_posts.sql")).execute(conn.handle().conn()).await?;
            let group = uuid::Uuid::new_v4();
            let file = fixture.file(group);
            repository::insert(&mut conn.handle(), &file).await?;
            drop(conn);
            let container = std::sync::Arc::new(context.container().await?);
            let claims = UserClaims::new(fixture.user_id, UserStatus::Active, vec!["board.data.write".into()]);
            let token = container.jwt_token_service.issue(&fixture.user_id.to_string(), claims.try_into()?)?.token;
            let router = crate::app::http::router(container.clone())?;
            let body = serde_json::json!({"board_slug":"data", "title":"title", "content":"body", "attachments":{"group_id":group,"attach_file_ids":[file.file_id],"detach_file_ids":[]}});
            let response = router.clone().oneshot(Request::post("/api/board/posts").header("Authorization", format!("Bearer {token}")).header("Content-Type", "application/json").body(Body::from(body.to_string()))?).await?;
            assert_eq!(response.status(), StatusCode::OK);
            let value: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
            let id = value["data"]["id"].as_i64().unwrap();
            let response = router.clone().oneshot(Request::get(format!("/api/board/posts/{id}")).body(Body::empty())?).await?;
            let detail: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
            assert_eq!(detail["data"]["attachments"][0]["original_filename"], "report.txt");
            let reader = UserClaims::new(fixture.user_id + 100, UserStatus::Active, vec![]);
            assert!(container.file_resource_access.decide(&reader, "board.post", &id.to_string(), Action::Read).await?);
            assert!(!container.file_resource_access.decide(&reader, "board.post", &id.to_string(), Action::Edit).await?);
            let owner = UserClaims::new(fixture.user_id, UserStatus::Active, vec![]);
            assert!(container.file_resource_access.decide(&owner, "board.post", &id.to_string(), Action::Edit).await?);
            let mut conn = context.db.conn().await?;
            assert_eq!(repository::find_by_id(&mut conn.handle(), file.file_id).await?.unwrap().status, FileStatus::Attached);
            drop(conn);
            let invalid = serde_json::json!({"board_slug":"data", "title":"bad", "content":"body", "attachments":{"group_id":uuid::Uuid::new_v4(),"attach_file_ids":[uuid::Uuid::new_v4()],"detach_file_ids":[]}});
            let response = router.clone().oneshot(Request::post("/api/board/posts").header("Authorization", format!("Bearer {token}")).header("Content-Type", "application/json").body(Body::from(invalid.to_string()))?).await?;
            assert_ne!(response.status(), StatusCode::OK);
            let mut conn = context.db.conn().await?;
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM conbira_posts").fetch_one(conn.handle().conn()).await?;
            assert_eq!(count, 1);
            drop(conn);
            let update = serde_json::json!({"title":"edited", "content":"body", "attachments":{"group_id":group,"attach_file_ids":[],"detach_file_ids":[file.file_id]}});
            let response = router.oneshot(Request::put(format!("/api/board/posts/{id}")).header("Authorization", format!("Bearer {token}")).header("Content-Type", "application/json").body(Body::from(update.to_string()))?).await?;
            assert_eq!(response.status(), StatusCode::OK);
            let mut conn = context.db.conn().await?;
            assert_eq!(repository::find_by_id(&mut conn.handle(), file.file_id).await?.unwrap().status, FileStatus::Deleted);
            Ok(())
        }).await
    }
}
