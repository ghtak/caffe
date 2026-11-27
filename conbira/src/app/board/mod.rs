pub mod entity;
mod feature;
pub mod repository;

pub fn router() -> axum::Router<std::sync::Arc<crate::app::Container>> {
    axum::Router::new()
        .route(
            "/posts/{id}",
            axum::routing::get(feature::post_read::endpoint::handler),
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
