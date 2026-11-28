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
            "/posts/{id}/attachments/{asset_id}",
            axum::routing::get(feature::post_attachment_download::endpoint::handler)
                .layer(axum::middleware::map_response(no_store)),
        )
        .route(
            "/posts/{id}",
            axum::routing::get(feature::post_read::endpoint::handler)
                .put(feature::post_update::endpoint::handler),
        )
        .route(
            "/posts/attachments/upload",
            axum::routing::post(feature::post_upload::endpoint::handler)
                .layer(axum::middleware::map_response(no_store)),
        )
        .route(
            "/posts",
            axum::routing::get(feature::post_search::endpoint::handler)
                .post(feature::post_create::endpoint::handler),
        )
}

async fn no_store(mut response: axum::response::Response) -> axum::response::Response {
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
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

#[derive(serde::Deserialize)]
pub struct AttachmentChanges {
    pub attach_file_ids: Vec<uuid::Uuid>,
    pub detach_file_ids: Vec<uuid::Uuid>,
}

async fn change_attachments(
    tx: &mut crate::app::db::Handle<'_>,
    service: &crate::app::file_asset::service::FileAssetService,
    caller_id: i64,
    post: &entity::Post,
    changes: AttachmentChanges,
) -> crate::Result<()> {
    use crate::app::file_asset::service::FileAssetStatus;
    use crate::{Code, Error};
    if post.board_slug != "data" {
        return Err(Error::new(
            Code::InvalidRequest,
            "Attachments are not allowed on this board",
        )
        .at(crate::here!()));
    }
    for id in changes.attach_file_ids {
        if !service.attach_owned(tx, id, caller_id).await? {
            return Err(Error::new(
                Code::InvalidRequest,
                "File asset is not an owned pending upload",
            )
            .at(crate::here!()));
        }
        repository::post_attachment::insert(tx, post.id, id).await?;
    }
    for id in changes.detach_file_ids {
        if !repository::post_attachment::remove(tx, post.id, id).await?
            || !service
                .change_status(tx, id, FileAssetStatus::Attached, FileAssetStatus::Deleted)
                .await?
        {
            return Err(
                Error::new(Code::InvalidRequest, "Post attachment cannot be removed")
                    .at(crate::here!()),
            );
        }
    }
    Ok(())
}
