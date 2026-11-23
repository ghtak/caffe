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
        .nest("/api/rbac", super::rbac::router())
        .nest("/api/auth", super::auth::router());

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

        #[tokio::test]
        async fn resource_routes_validate_request_bodies() -> anyhow::Result<()> {
            let router = super::super::router(crate::test_support::container().await?)?;
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
        }

        #[tokio::test]
        async fn router_returns_bearer_challenge_for_missing_and_invalid_credentials()
        -> anyhow::Result<()> {
            let container = crate::test_support::container().await?;
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
                    // 선택적 및 필수 추출 경로 모두 인증 실패에 Bearer 챌린지를
                    // 반환한다.
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
        }
    }
}
