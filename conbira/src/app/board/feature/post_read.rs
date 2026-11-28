pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::board::entity::PostDetail;
    use crate::app::board::repository::{post, post_attachment};
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::ApiResult;
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    #[derive(serde::Serialize)]
    pub struct ResponseData {
        #[serde(flatten)]
        post: PostDetail,
        attachments: Vec<crate::app::file_asset::FileAssetSummary>,
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiPath(id): ApiPath<i64>,
    ) -> ApiResult<ResponseData> {
        let mut conn = container.db.acquire_conn(crate::here!()).await?;
        let post = post::find_by_id(&mut conn.handle(), id)
            .await?
            .ok_or_else(|| Error::new(Code::NotFound, "Post not found").at(crate::here!()))?;
        let attachments = post_attachment::list(&mut conn.handle(), id).await?;
        Ok(ResponseData { post, attachments }.into())
    }
}

#[cfg(test)]
mod tests {
    mod contract {
        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt as _;

        #[tokio::test]
        async fn reads_post_content_and_author_without_login_and_excludes_deleted()
        -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../../migrations/0020_posts.sql")).execute(conn.handle().conn()).await?;
                sqlx::raw_sql("INSERT INTO conbira_auth_users (nickname, status) VALUES ('writer', 'active');
                    INSERT INTO conbira_posts (board_slug, author_id, title) VALUES ('free', 1, 'title');
                    INSERT INTO conbira_post_contents (post_id, content) VALUES (1, '# text');")
                    .execute(conn.handle().conn()).await?;
                drop(conn);
                let router = crate::app::http::router(std::sync::Arc::new(context.container().await?))?;
                let response = router.clone().oneshot(Request::get("/api/board/posts/1").body(Body::empty())?).await?;
                let status = response.status();
                let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                // 인증 없이 제목·닉네임·원본 본문을 반환한다.
                assert_eq!(status, StatusCode::OK);
                assert_eq!(body["data"]["title"], "title");
                assert_eq!(body["data"]["author_nickname"], "writer");
                assert_eq!(body["data"]["content"], "# text");
                let mut conn = context.db.conn().await?;
                sqlx::query("UPDATE conbira_posts SET deleted_at = CURRENT_TIMESTAMP WHERE id = 1").execute(conn.handle().conn()).await?;
                drop(conn);
                for id in [1, 999] {
                    let response = router.clone().oneshot(Request::get(format!("/api/board/posts/{id}")).body(Body::empty())?).await?;
                    // 삭제되었거나 존재하지 않는 글은 반환하지 않는다.
                    assert_eq!(response.status(), StatusCode::NOT_FOUND);
                }
                Ok(())
            }).await
        }
    }
}
