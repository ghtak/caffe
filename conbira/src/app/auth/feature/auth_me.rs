use axum::response::Response;

use crate::app::auth::claims::AuthClaims;
use crate::app::auth::extractor::bearer_rejection;
use crate::app::http::response::ApiResponse;

pub async fn handler(
    claims: Option<AuthClaims>,
) -> Result<ApiResponse<espr::jwt::Claims>, Response> {
    let AuthClaims(claims) =
        claims.ok_or_else(|| bearer_rejection("Authentication required", "auth.me"))?;
    Ok(ApiResponse::new(claims))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use axum::response::IntoResponse;
    use tower::ServiceExt as _;

    use super::*;
    use crate::app::auth::claims::UserClaims;

    #[tokio::test]
    async fn returns_claims_and_rejects_missing_authentication() -> anyhow::Result<()> {
        let claims = espr::jwt::Claims {
            sub: "42".into(),
            iss: "issuer".into(),
            aud: "audience".into(),
            iat: 100,
            exp: 200,
            user: serde_json::json!({"id": 42, "status": "active"})
                .as_object()
                .unwrap()
                .clone(),
        };
        let expected = serde_json::to_value(&claims)?;
        let response = handler(Some(AuthClaims(claims)))
            .await
            .unwrap()
            .into_response();
        let status = response.status();
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
        // 응답 데이터는 추출된 클레임 전체와 일치한다.
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"], expected);
        // 인증 정보가 없는 요청은 401로 거부한다.
        let response = handler(None).await.unwrap_err();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers()[axum::http::header::WWW_AUTHENTICATE],
            "Bearer"
        );
        Ok(())
    }

    #[tokio::test]
    async fn router_returns_bearer_challenge_for_missing_and_invalid_credentials()
    -> anyhow::Result<()> {
        let dir = std::env::temp_dir().join(format!("conbira-auth-me-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir)?;
        let mut config = crate::config::Config::load(
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml")),
            Some(&dir.join("missing.env")),
        )?;
        config.database = crate::test_support::database_config(1);
        config.cache.url = std::env::var("TESTENV_CACHE_URL")
            .unwrap_or_else(|_| "redis://192.168.1.20:36379".into());
        config.jwt.ed25519.key_path = dir.join("private.pem");
        config.jwt.ed25519.public_key_path = dir.join("public.pem");
        espr::jwt::ed25519::generate(&config.jwt.ed25519)?;
        let container = Arc::new(crate::app::Container::new(&config).await?);
        std::fs::remove_dir_all(dir)?;
        let token = container
            .jwt_token_service
            .issue("test-user", Default::default())?
            .token;
        async fn required(AuthClaims(claims): AuthClaims) -> String { claims.sub }
        let router = crate::app::http::router(Arc::clone(&container))?.route(
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

    async fn user_claims_router() -> anyhow::Result<(axum::Router, Arc<crate::app::Container>)> {
        let dir =
            std::env::temp_dir().join(format!("conbira-user-claims-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir)?;
        let mut config = crate::config::Config::load(
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml")),
            Some(&dir.join("missing.env")),
        )?;
        config.database = crate::test_support::database_config(1);
        config.cache.url = std::env::var("TESTENV_CACHE_URL")
            .unwrap_or_else(|_| "redis://192.168.1.20:36379".into());
        config.jwt.ed25519.key_path = dir.join("private.pem");
        config.jwt.ed25519.public_key_path = dir.join("public.pem");
        espr::jwt::ed25519::generate(&config.jwt.ed25519)?;
        let container = Arc::new(crate::app::Container::new(&config).await?);
        std::fs::remove_dir_all(dir)?;

        async fn required(user: UserClaims) -> String { user.id.to_string() }
        async fn optional(user: Option<UserClaims>) -> String {
            user.map_or_else(|| "anonymous".into(), |user| user.id.to_string())
        }
        let router = axum::Router::new()
            .route("/required", axum::routing::get(required))
            .route("/optional", axum::routing::get(optional))
            .with_state(Arc::clone(&container));
        Ok((router, container))
    }

    #[tokio::test]
    async fn user_claims_without_header_are_required_or_anonymous() -> anyhow::Result<()> {
        let (router, _) = user_claims_router().await?;
        let required = router
            .clone()
            .oneshot(Request::builder().uri("/required").body(Body::empty())?)
            .await?;
        // 필수 추출은 인증이 없으면 Bearer 챌린지와 401을 반환한다.
        assert_eq!(required.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(required.headers()[header::WWW_AUTHENTICATE], "Bearer");

        let optional = router
            .oneshot(Request::builder().uri("/optional").body(Body::empty())?)
            .await?;
        // 선택 추출은 인증이 없으면 익명으로 처리한다.
        assert_eq!(optional.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(optional.into_body(), 4096).await?.as_ref(),
            b"anonymous"
        );
        Ok(())
    }

    #[tokio::test]
    async fn user_claims_extractors_accept_matching_user() -> anyhow::Result<()> {
        let (router, container) = user_claims_router().await?;
        let user = serde_json::json!({"id": 42, "status": "active"});
        let token = container
            .jwt_token_service
            .issue("42", user.as_object().unwrap().clone())?
            .token;
        for path in ["/required", "/optional"] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header(header::AUTHORIZATION, format!("Bearer {token}"))
                        .body(Body::empty())?,
                )
                .await?;
            // 두 추출기 모두 검증된 사용자 ID를 전달한다.
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(
                to_bytes(response.into_body(), 4096).await?.as_ref(),
                b"42",
                "{path}"
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn user_claims_extractors_reject_invalid_tokens_and_user_claims() -> anyhow::Result<()> {
        let (router, container) = user_claims_router().await?;
        for (subject, user) in [
            ("43", serde_json::json!({"id": 42, "status": "active"})),
            ("42", serde_json::json!({})),
            ("42", serde_json::json!({"id": 42, "status": "unknown"})),
            ("42", serde_json::json!({"id": "42", "status": "active"})),
        ] {
            let token = container
                .jwt_token_service
                .issue(subject, user.as_object().unwrap().clone())?
                .token;
            for path in ["/required", "/optional"] {
                let response = router
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri(path)
                            .header(header::AUTHORIZATION, format!("Bearer {token}"))
                            .body(Body::empty())?,
                    )
                    .await?;
                // 불일치하거나 잘못된 사용자 클레임은 Bearer 챌린지와 401을
                // 반환한다.
                assert_eq!(
                    response.status(),
                    StatusCode::UNAUTHORIZED,
                    "{path}: {user}"
                );
                assert_eq!(
                    response.headers()[header::WWW_AUTHENTICATE],
                    "Bearer",
                    "{path}: {user}"
                );
            }
        }
        for path in ["/required", "/optional"] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header(header::AUTHORIZATION, "Bearer invalid")
                        .body(Body::empty())?,
                )
                .await?;
            // 잘못된 토큰은 Bearer 챌린지와 401을 반환한다.
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
            assert_eq!(
                response.headers()[header::WWW_AUTHENTICATE],
                "Bearer",
                "{path}"
            );
        }
        Ok(())
    }
}
