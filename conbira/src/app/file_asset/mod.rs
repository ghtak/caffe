pub mod config;

pub mod service;

mod repository;

pub use repository::{FileAssetSummary, find_by_ids};

mod feature;

pub fn router() -> axum::Router<std::sync::Arc<crate::app::Container>> {
    axum::Router::new()
        .route(
            "/file-assets",
            axum::routing::get(feature::file_asset_list::endpoint::handler)
                .post(feature::file_asset_upload::endpoint::handler),
        )
        .route(
            "/file-assets/cleanup",
            axum::routing::post(feature::file_asset_cleanup::endpoint::handler),
        )
        .route(
            "/file-assets/{id}",
            axum::routing::delete(feature::file_asset_delete::endpoint::handler),
        )
        .route(
            "/file-assets/{id}/attach",
            axum::routing::post(feature::file_asset_attach::endpoint::handler),
        )
        .route(
            "/file-assets/{id}/download",
            axum::routing::get(feature::file_asset_download::endpoint::handler),
        )
        .layer(axum::middleware::map_response(no_store))
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

        use crate::app::auth::claims::UserClaims;
        use crate::app::auth::entity::UserStatus;

        #[tokio::test]
        async fn list_requires_permission_and_returns_paginated_metadata() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0014_file_asset.sql")).execute(conn.handle().conn()).await?;
                let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('reader') RETURNING id").fetch_one(conn.handle().conn()).await?;
                for name in ["first.txt", "second.txt"] {
                    sqlx::query("INSERT INTO conbira_file_assets (id, status, uploaded_by, s3_bucket, s3_object_key, original_filename, size_bytes, content_type) VALUES ($1, 'pending', $2, 'bucket', $3, $3, 4, 'text/plain')").bind(uuid::Uuid::new_v4()).bind(user).bind(name).execute(conn.handle().conn()).await?;
                }
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let issue = |permissions: Vec<String>| -> anyhow::Result<String> { Ok(container.jwt_token_service.issue(&user.to_string(), UserClaims::new(user, UserStatus::Active, permissions).try_into()?)?.token) };
                let reader = issue(vec!["file_asset.list.read".into()])?;
                let other = issue(vec![])?;
                let router = crate::app::http::router(container)?;
                let request = |token: &str| Request::get("/api/file-assets?page_size=1").header("Authorization", format!("Bearer {token}")).body(Body::empty());
                let denied = router.clone().oneshot(request(&other)?).await?;
                assert_eq!(denied.status(), StatusCode::FORBIDDEN);
                let response = router.oneshot(request(&reader)?).await?;
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(response.headers()["cache-control"], "no-store");
                let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
                assert_eq!(body["data"]["has_next"], true);
                assert!(body["data"]["items"][0].get("s3_object_key").is_none());
                Ok(())
            }).await
        }

        #[tokio::test]
        async fn upload_requires_manage_and_uses_authenticated_user() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0014_file_asset.sql")).execute(conn.handle().conn()).await?;
                let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('writer') RETURNING id").fetch_one(conn.handle().conn()).await?;
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let issue = |permissions: Vec<String>| -> anyhow::Result<String> { Ok(container.jwt_token_service.issue(&user.to_string(), UserClaims::new(user, UserStatus::Active, permissions).try_into()?)?.token) };
                let manager = issue(vec!["file_asset.manage".into()])?;
                let reader = issue(vec!["file_asset.list.read".into()])?;
                let router = crate::app::http::router(container)?;
                let body = serde_json::json!({"original_filename":"report.txt", "size_bytes":4, "content_type":"text/plain", "uploaded_by":999}).to_string();
                let request = |token: &str| Request::post("/api/file-assets").header("Authorization", format!("Bearer {token}")).header("Content-Type", "application/json").body(Body::from(body.clone()));
                assert_eq!(router.clone().oneshot(request(&reader)?).await?.status(), StatusCode::FORBIDDEN);
                let response = router.oneshot(request(&manager)?).await?;
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(response.headers()["cache-control"], "no-store");
                let result: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                assert!(result["data"]["upload_url"].as_str().unwrap().contains("X-Amz-Signature="));
                let mut conn = context.db.conn().await?;
                let stored: i64 = sqlx::query_scalar("SELECT uploaded_by FROM conbira_file_assets WHERE id=$1").bind(uuid::Uuid::parse_str(result["data"]["id"].as_str().unwrap())?).fetch_one(conn.handle().conn()).await?;
                assert_eq!(stored, user);
                Ok(())
            }).await
        }
    }
}

#[cfg(test)]
mod download_tests {
    mod contract {
        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt as _;

        use crate::app::auth::claims::UserClaims;
        use crate::app::auth::entity::UserStatus;

        #[tokio::test]
        async fn download_requires_read_permission_and_excludes_deleted() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0014_file_asset.sql")).execute(conn.handle().conn()).await?;
                let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('reader') RETURNING id").fetch_one(conn.handle().conn()).await?;
                let id = uuid::Uuid::new_v4();
                sqlx::query("INSERT INTO conbira_file_assets (id, status, uploaded_by, s3_bucket, s3_object_key, original_filename, size_bytes, content_type) VALUES ($1, 'attached', $2, 'bucket', 'key', 'report.txt', 4, 'text/plain')").bind(id).bind(user).execute(conn.handle().conn()).await?;
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let issue = |permissions: Vec<String>| -> anyhow::Result<String> { Ok(container.jwt_token_service.issue(&user.to_string(), UserClaims::new(user, UserStatus::Active, permissions).try_into()?)?.token) };
                let reader = issue(vec!["file_asset.list.read".into()])?;
                let other = issue(vec!["file_asset.manage".into()])?;
                let router = crate::app::http::router(container)?;
                let request = |token: &str| Request::get(format!("/api/file-assets/{id}/download")).header("Authorization", format!("Bearer {token}")).body(Body::empty());
                assert_eq!(router.clone().oneshot(request(&other)?).await?.status(), StatusCode::FORBIDDEN);
                let response = router.clone().oneshot(request(&reader)?).await?;
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(response.headers()["cache-control"], "no-store");
                let result: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
                assert!(result["data"]["download_url"].as_str().unwrap().contains("X-Amz-Signature="));
                let mut conn = context.db.conn().await?;
                sqlx::query("UPDATE conbira_file_assets SET status='deleted', deleted_at=CURRENT_TIMESTAMP WHERE id=$1").bind(id).execute(conn.handle().conn()).await?;
                drop(conn);
                assert_eq!(router.oneshot(request(&reader)?).await?.status(), StatusCode::NOT_FOUND);
                Ok(())
            }).await
        }
    }
}

#[cfg(test)]
mod delete_tests {
    mod contract {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt as _;

        use crate::app::auth::claims::UserClaims;
        use crate::app::auth::entity::UserStatus;

        #[tokio::test]
        async fn delete_requires_manage_and_marks_active_assets() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0014_file_asset.sql")).execute(conn.handle().conn()).await?;
                let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('manager') RETURNING id").fetch_one(conn.handle().conn()).await?;
                let pending = uuid::Uuid::new_v4();
                let attached = uuid::Uuid::new_v4();
                for (id, status) in [(pending, "pending"), (attached, "attached")] {
                    sqlx::query("INSERT INTO conbira_file_assets (id, status, uploaded_by, s3_bucket, s3_object_key, original_filename, size_bytes, content_type) VALUES ($1, $2, $3, 'bucket', $1::text, 'report.txt', 4, 'text/plain')").bind(id).bind(status).bind(user).execute(conn.handle().conn()).await?;
                }
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let issue = |permissions: Vec<String>| -> anyhow::Result<String> { Ok(container.jwt_token_service.issue(&user.to_string(), UserClaims::new(user, UserStatus::Active, permissions).try_into()?)?.token) };
                let manager = issue(vec!["file_asset.manage".into()])?;
                let reader = issue(vec!["file_asset.list.read".into()])?;
                let router = crate::app::http::router(container)?;
                let request = |token: &str, id: uuid::Uuid| Request::delete(format!("/api/file-assets/{id}")).header("Authorization", format!("Bearer {token}")).body(Body::empty());
                assert_eq!(router.clone().oneshot(request(&reader, pending)?).await?.status(), StatusCode::FORBIDDEN);
                assert_eq!(router.clone().oneshot(request(&manager, attached)?).await?.status(), StatusCode::OK);
                assert_eq!(router.clone().oneshot(request(&manager, uuid::Uuid::new_v4())?).await?.status(), StatusCode::NOT_FOUND);
                assert_eq!(router.clone().oneshot(request(&manager, pending)?).await?.status(), StatusCode::OK);
                assert_eq!(router.oneshot(request(&manager, pending)?).await?.status(), StatusCode::OK);
                let mut conn = context.db.conn().await?;
                let state: (String, bool) = sqlx::query_as("SELECT status, deleted_at IS NOT NULL FROM conbira_file_assets WHERE id=$1").bind(pending).fetch_one(conn.handle().conn()).await?;
                assert_eq!(state, ("deleted".to_owned(), true));
                let status: String = sqlx::query_scalar("SELECT status FROM conbira_file_assets WHERE id=$1").bind(attached).fetch_one(conn.handle().conn()).await?;
                assert_eq!(status, "deleted");
                Ok(())
            }).await
        }
    }
}

#[cfg(test)]
mod attach_tests {
    mod contract {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt as _;

        use crate::app::auth::claims::UserClaims;
        use crate::app::auth::entity::UserStatus;

        #[tokio::test]
        async fn attach_requires_manage_and_pending_status() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0014_file_asset.sql")).execute(conn.handle().conn()).await?;
                let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('manager') RETURNING id").fetch_one(conn.handle().conn()).await?;
                let id = uuid::Uuid::new_v4();
                sqlx::query("INSERT INTO conbira_file_assets (id, status, uploaded_by, s3_bucket, s3_object_key, original_filename, size_bytes, content_type) VALUES ($1, 'pending', $2, 'bucket', 'key', 'report.txt', 4, 'text/plain')").bind(id).bind(user).execute(conn.handle().conn()).await?;
                drop(conn);
                let (service, _server) = context.file_asset_service(4).await?;
                let mut container = context.container().await?;
                container.file_asset_service = service;
                let container = std::sync::Arc::new(container);
                let issue = |permissions: Vec<String>| -> anyhow::Result<String> { Ok(container.jwt_token_service.issue(&user.to_string(), UserClaims::new(user, UserStatus::Active, permissions).try_into()?)?.token) };
                let reader = issue(vec!["file_asset.list.read".into()])?;
                let manager = issue(vec!["file_asset.manage".into()])?;
                let router = crate::app::http::router(container)?;
                let request = |token: &str| Request::post(format!("/api/file-assets/{id}/attach")).header("Authorization", format!("Bearer {token}")).body(Body::empty());
                assert_eq!(router.clone().oneshot(request(&reader)?).await?.status(), StatusCode::FORBIDDEN);
                assert_eq!(router.clone().oneshot(request(&manager)?).await?.status(), StatusCode::OK);
                assert_eq!(router.oneshot(request(&manager)?).await?.status(), StatusCode::BAD_REQUEST);
                let mut conn = context.db.conn().await?;
                let status: String = sqlx::query_scalar("SELECT status FROM conbira_file_assets WHERE id=$1").bind(id).fetch_one(conn.handle().conn()).await?;
                assert_eq!(status, "attached");
                Ok(())
            }).await
        }
    }
}

#[cfg(test)]
mod cleanup_tests {
    mod contract {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt as _;

        use crate::app::auth::claims::UserClaims;
        use crate::app::auth::entity::UserStatus;

        #[tokio::test]
        async fn cleanup_requires_manage_and_accepts_background_work() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0014_file_asset.sql")).execute(conn.handle().conn()).await?;
                let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('cleanup manager') RETURNING id").fetch_one(conn.handle().conn()).await?;
                drop(conn);
                let container = std::sync::Arc::new(context.container().await?);
                let issue = |permissions: Vec<String>| -> anyhow::Result<String> { Ok(container.jwt_token_service.issue(&user.to_string(), UserClaims::new(user, UserStatus::Active, permissions).try_into()?)?.token) };
                let reader = issue(vec!["file_asset.list.read".into()])?;
                let manager = issue(vec!["file_asset.manage".into()])?;
                let router = crate::app::http::router(container)?;
                let request = |token: &str| Request::post("/api/file-assets/cleanup").header("Authorization", format!("Bearer {token}")).body(Body::empty());
                assert_eq!(router.clone().oneshot(request(&reader)?).await?.status(), StatusCode::FORBIDDEN);
                assert_eq!(router.oneshot(request(&manager)?).await?.status(), StatusCode::ACCEPTED);
                Ok(())
            }).await
        }
    }
}
