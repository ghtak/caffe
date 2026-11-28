pub mod entity;
mod feature;
pub mod repository;

pub fn router() -> axum::Router<std::sync::Arc<crate::app::Container>> {
    axum::Router::new()
        .route(
            "/comments/{id}",
            axum::routing::put(feature::comment_update::endpoint::handler)
                .delete(feature::comment_delete::endpoint::handler),
        )
        .route(
            "/posts/{id}/comments",
            axum::routing::get(feature::comment_list::endpoint::handler)
                .post(feature::comment_create::endpoint::handler),
        )
        .route(
            "/posts/{id}",
            axum::routing::get(feature::post_read::endpoint::handler)
                .put(feature::post_update::endpoint::handler),
        )
        .route(
            "/posts",
            axum::routing::get(feature::post_search::endpoint::handler)
                .post(feature::post_create::endpoint::handler),
        )
}

#[cfg(test)]
mod tests {
    mod contract {
        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt as _;

        #[tokio::test]
        async fn searches_title_or_content_with_author_and_pagination() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0020_posts.sql"))
                    .execute(conn.handle().conn()).await?;
                sqlx::raw_sql(
                    "INSERT INTO conbira_auth_users (nickname, status) VALUES ('first', 'active'), ('second', 'active');
                     INSERT INTO conbira_posts (board_slug, author_id, title) VALUES
                     ('free', 1, 'needle title'), ('free', 1, 'body match'),
                     ('notice', 2, 'needle other author'), ('free', 1, 'unrelated'),
                     ('free', 1, 'needle deleted');
                     INSERT INTO conbira_post_contents (post_id, content) VALUES (2, 'contains needle here');
                     UPDATE conbira_posts SET deleted_at = CURRENT_TIMESTAMP WHERE id = 5;"
                ).execute(conn.handle().conn()).await?;
                drop(conn);
                let router = crate::app::http::router(std::sync::Arc::new(context.container().await?))?;
                for (page, expected_id, has_next) in [(1, 2, true), (2, 1, false)] {
                    let response = router.clone().oneshot(Request::get(format!(
                        "/api/board/posts?board_slug=free&search=needle&author_id=1&page={page}&page_size=1"
                    )).body(Body::empty())?).await?;
                    let status = response.status();
                    let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                    // 제목 또는 본문이 일치하고 작성자가 같은 활성 글만 최신순으로 페이지에 포함한다.
                    assert_eq!(status, StatusCode::OK, "{body}");
                    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
                    assert_eq!(body["data"]["items"][0]["id"], expected_id);
                    assert_eq!(body["data"]["items"][0]["author_nickname"], "first");
                    assert_eq!(body["data"]["has_next"], has_next);
                }
                let response = router.oneshot(Request::get("/api/board/posts?board_slug=free&page_size=100").body(Body::empty())?).await?;
                let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                // 검색 조건을 생략하면 삭제 글을 제외한 전체 목록을 반환한다.
                assert_eq!(body["data"]["items"].as_array().unwrap().len(), 3);
                Ok(())
            }).await
        }

        #[tokio::test]
        async fn rejects_invalid_pagination() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(1, |context| async move {
                let router =
                    crate::app::http::router(std::sync::Arc::new(context.container().await?))?;
                for query in ["page=0", "page_size=0", "page_size=101"] {
                    let response = router
                        .clone()
                        .oneshot(
                            Request::get(format!("/api/board/posts?board_slug=free&{query}"))
                                .body(Body::empty())?,
                        )
                        .await?;
                    // 페이지는 1 이상, 페이지 크기는 1~100만 허용한다.
                    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
                }
                let response = router
                    .oneshot(Request::get("/api/board/posts").body(Body::empty())?)
                    .await?;
                // board_slug는 필수 요청 값이다.
                assert_eq!(response.status(), StatusCode::BAD_REQUEST);
                Ok(())
            })
            .await
        }
    }
}

/// 게시글에 연결된 파일의 접근 정책.
pub struct PostFileAccess {
    pub db: std::sync::Arc<crate::app::db::Database>,
}

#[async_trait::async_trait]
impl crate::app::files::access::ResourceAccessPort for PostFileAccess {
    async fn decide(
        &self,
        caller: &crate::app::auth::claims::UserClaims,
        kind: &str,
        key: &str,
        action: crate::app::files::access::Action,
    ) -> crate::Result<bool> {
        use crate::app::DatabaseExt as _;
        use crate::app::files::access::Action;
        if kind != "board.post" {
            return Ok(false);
        }
        let Ok(id) = key.parse::<i64>() else {
            return Ok(false);
        };
        let mut conn = self.db.acquire_conn(crate::here!()).await?;
        let post = repository::post::find_by_id(&mut conn.handle(), id).await?;
        Ok(post.is_some_and(|p| {
            p.summary.post.board_slug == "data"
                && (action == Action::Read || p.summary.post.author_id == caller.id)
        }))
    }
}

#[derive(serde::Deserialize)]
pub struct AttachmentChanges {
    pub group_id: uuid::Uuid,
    pub attach_file_ids: Vec<uuid::Uuid>,
    pub detach_file_ids: Vec<uuid::Uuid>,
}

// 게시글 작성자 확인을 마친 트랜잭션 내부에서만 사용한다.
struct AuthorizedPost;
#[async_trait::async_trait]
impl crate::app::files::access::ResourceAccessPort for AuthorizedPost {
    async fn decide(
        &self,
        _: &crate::app::auth::claims::UserClaims,
        kind: &str,
        _: &str,
        _: crate::app::files::access::Action,
    ) -> crate::Result<bool> {
        Ok(kind == "board.post")
    }
}

async fn change_attachments(
    tx: &mut crate::app::db::Handle<'_>,
    caller: crate::app::auth::claims::UserClaims,
    post: &mut entity::Post,
    changes: AttachmentChanges,
    ttl: u64,
) -> crate::Result<()> {
    if post.board_slug != "data"
        || post
            .attachment_group_id
            .is_some_and(|id| id != changes.group_id)
        || (post.attachment_group_id.is_none() && changes.attach_file_ids.is_empty())
    {
        return Err(crate::Error::new(
            crate::Code::InvalidRequest,
            "Invalid post attachment group",
        )
        .at(crate::here!()));
    }
    crate::app::files::file_group_attach::execute_in_tx(
        tx,
        &AuthorizedPost,
        ttl,
        crate::app::files::file_group_attach::Command {
            caller,
            group_id: changes.group_id,
            resource_type: "board.post".into(),
            resource_key: post.id.to_string(),
            attach_file_ids: changes.attach_file_ids,
            detach_file_ids: changes.detach_file_ids,
        },
    )
    .await?;
    use crate::error::ResultExt as _;
    sqlx::query("UPDATE conbira_posts SET attachment_group_id = $2 WHERE id = $1")
        .bind(post.id)
        .bind(changes.group_id)
        .execute(tx.conn())
        .await
        .map_err_internal("Failed to update attachment group", crate::here!())?;
    post.attachment_group_id = Some(changes.group_id);
    Ok(())
}
