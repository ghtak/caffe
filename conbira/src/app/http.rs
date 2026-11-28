use std::sync::Arc;

use axum::http::StatusCode;
use axum::routing::get;
use espr::validate::Validate;
use tower_http::trace::TraceLayer;

use crate::app::Container;
use crate::error::ResultExt as _;

pub mod extract;
pub mod response;

pub fn router(container: Arc<Container>) -> crate::Result<axum::Router> {
    let router = axum::Router::new()
        .route("/api/health", get(|| async { StatusCode::OK }))
        .nest("/api/samples", super::sample::router())
        .nest("/api/board", super::board::router())
        .nest("/api/rbac", super::rbac::router())
        .nest("/api/auth", super::auth::router())
        .nest("/api", super::files::router())
        .nest("/api", super::file_asset::router());

    let router = match &container.config.http.static_files {
        Some(static_files) => {
            static_files
                .validate()
                .map_err_internal("Failed to validate static files", crate::here!())?;
            router.merge(static_files.as_router())
        }
        None => router,
    };
    let router =
        router.layer(TraceLayer::new_for_http().make_span_with(espr::http::make_request_span));

    Ok(router.with_state(container))
}

#[cfg(test)]
mod tests {
    mod contract {
        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode, header};
        use tower::ServiceExt as _;

        use crate::app::auth::claims::AuthClaims;

        async fn auth_router(
            context: &crate::test_support::TestContext,
        ) -> anyhow::Result<(axum::Router, String)> {
            let container = std::sync::Arc::new(context.container().await?);
            let mut conn = container.db.conn().await?;
            let user_id: i64 = sqlx::query_scalar(
                "INSERT INTO conbira_auth_users (nickname, status) VALUES ('user', 'active') RETURNING id",
            )
            .fetch_one(conn.handle().conn())
            .await?;
            let mut tx = conn.begin().await?;
            let token = container
                .refresh_token_service
                .issue(&mut tx.handle(), user_id)
                .await?
                .token;
            tx.commit().await?;
            drop(conn);
            Ok((super::super::router(container)?, token))
        }

        #[tokio::test]
        async fn logout_without_bearer_revokes_token_and_is_idempotent() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(1, |context| async move {
                let (router, token) = auth_router(&context).await?;
                for _ in 0..2 {
                    let response = router
                        .clone()
                        .oneshot(
                            Request::post("/api/auth/logout")
                                .header(header::CONTENT_TYPE, "application/json")
                                .body(Body::from(
                                    serde_json::json!({"refresh_token": token}).to_string(),
                                ))?,
                        )
                        .await?;
                    // Bearer 없이도 로그아웃과 재시도가 성공한다.
                    assert_eq!(response.status(), StatusCode::OK);
                    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
                    let body: serde_json::Value =
                        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
                    assert_eq!(body, serde_json::json!({"code": "OK"}));
                }
                let response = router
                    .oneshot(
                        Request::post("/api/auth/refresh")
                            .header(header::CONTENT_TYPE, "application/json")
                            .body(Body::from(
                                serde_json::json!({"refresh_token": token}).to_string(),
                            ))?,
                    )
                    .await?;
                // 폐기된 토큰으로는 갱신할 수 없다.
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
                assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn web_session_refresh_and_logout_use_http_only_cookie() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(1, |context| async move {
                let (router, token) = auth_router(&context).await?;
                let cookie = format!("__Host-conbira-refresh={token}");
                let refresh = router
                    .clone()
                    .oneshot(
                        Request::post("/api/auth/web/refresh")
                            .header("x-conbira-request", "1")
                            .header(header::COOKIE, &cookie)
                            .body(Body::empty())?,
                    )
                    .await?;
                assert_eq!(refresh.status(), StatusCode::OK);
                let set_cookie = refresh.headers()[header::SET_COOKIE].to_str()?;
                assert!(
                    set_cookie.contains("HttpOnly")
                        && set_cookie.contains("Secure")
                        && set_cookie.contains("SameSite=Strict")
                );
                let body: serde_json::Value =
                    serde_json::from_slice(&to_bytes(refresh.into_body(), 4096).await?)?;
                assert!(body["data"]["access_token"].is_string());
                assert!(body["data"].get("refresh_token").is_none());
                let logout = router
                    .clone()
                    .oneshot(
                        Request::post("/api/auth/web/logout")
                            .header("x-conbira-request", "1")
                            .header(header::COOKIE, &cookie)
                            .body(Body::empty())?,
                    )
                    .await?;
                assert_eq!(logout.status(), StatusCode::OK);
                assert!(
                    logout.headers()[header::SET_COOKIE]
                        .to_str()?
                        .contains("Max-Age=0")
                );
                let refresh = router
                    .oneshot(
                        Request::post("/api/auth/web/refresh")
                            .header("x-conbira-request", "1")
                            .header(header::COOKIE, cookie)
                            .body(Body::empty())?,
                    )
                    .await?;
                assert_eq!(refresh.status(), StatusCode::UNAUTHORIZED);
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn web_session_rejects_requests_without_csrf_header() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(1, |context| async move {
                let router = super::super::router(std::sync::Arc::new(context.container().await?))?;
                for path in ["/api/auth/web/refresh", "/api/auth/web/logout"] {
                    let response = router
                        .clone()
                        .oneshot(Request::post(path).body(Body::empty())?)
                        .await?;
                    assert_eq!(response.status(), StatusCode::FORBIDDEN);
                }
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn refresh_success_has_no_store_header() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(1, |context| async move {
                let (router, token) = auth_router(&context).await?;
                let response = router
                    .oneshot(
                        Request::post("/api/auth/refresh")
                            .header(header::CONTENT_TYPE, "application/json")
                            .body(Body::from(
                                serde_json::json!({"refresh_token": token}).to_string(),
                            ))?,
                    )
                    .await?;
                // 유효한 토큰의 갱신 응답은 캐시되지 않는다.
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
                let body: serde_json::Value =
                    serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
                assert_eq!(body["data"]["refresh_token"], token);
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn refresh_and_logout_reject_invalid_bodies_without_caching() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(1, |context| async move {
                let router = super::super::router(std::sync::Arc::new(context.container().await?))?;
                for path in ["/api/auth/refresh", "/api/auth/logout"] {
                    for body in ["{", "{}", r#"{"refresh_token":""}"#] {
                        let response = router
                            .clone()
                            .oneshot(
                                Request::post(path)
                                    .header(header::CONTENT_TYPE, "application/json")
                                    .body(Body::from(body))?,
                            )
                            .await?;
                        // 잘못된 본문은 공통 400 응답이며 캐시되지 않는다.
                        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}: {body}");
                        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
                        let result: serde_json::Value =
                            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
                        assert_eq!(result["code"], "INVALID_REQUEST");
                    }
                }
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn resource_routes_validate_request_bodies() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(1, |context| async move {
                let router = super::super::router(std::sync::Arc::new(context.container().await?))?;
                for path in [
                    "/api/samples",
                    "/api/auth/users/local",
                    "/api/auth/login/local",
                ] {
                    let response = router
                        .clone()
                        .oneshot(
                            Request::post(path)
                                .header(header::CONTENT_TYPE, "application/json")
                                .body(Body::from("{"))?,
                        )
                        .await?;
                    // 새 경로는 요청 본문을 검사하는 핸들러에 도달한다.
                    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
                }
                Ok(())
            })
            .await
        }

        #[tokio::test]
        async fn router_returns_bearer_challenge_for_missing_and_invalid_credentials()
        -> anyhow::Result<()> {
            crate::test_support::TestContext::run(1, |context| async move {
                let container = std::sync::Arc::new(context.container().await?);
                let token = container
                    .jwt_token_service
                    .issue("test-user", Default::default())?
                    .token;
                async fn required(AuthClaims(claims): AuthClaims) -> String { claims.sub }
                let router = super::super::router(container.clone())?.route(
                    "/required",
                    axum::routing::get(required).with_state(container),
                );
                for path in ["/api/auth/me", "/required"] {
                    for authorization in [None, Some("Bearer invalid")] {
                        let mut request = Request::builder().uri(path);
                        if let Some(value) = authorization {
                            request = request.header(header::AUTHORIZATION, value);
                        }
                        let response = router.clone().oneshot(request.body(Body::empty())?).await?;
                        // 선택적 및 필수 추출 경로 모두 인증 실패에 Bearer
                        // 챌린지를 반환한다.
                        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
                        assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer");
                    }
                }
                let response = router
                    .oneshot(
                        Request::builder()
                            .uri("/required")
                            .header(header::AUTHORIZATION, format!("Bearer {token}"))
                            .body(Body::empty())?,
                    )
                    .await?;
                // 유효한 토큰은 필수 추출기를 통과하고 클레임을 전달한다.
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(
                    to_bytes(response.into_body(), 4096).await?.as_ref(),
                    b"test-user"
                );
                Ok(())
            })
            .await
        }
    }
}
