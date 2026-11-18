use axum::response::Response;

use crate::app::auth::extractor::{AuthClaims, bearer_rejection};
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
        let router = crate::app::http::router(container)?;
        for authorization in [None, Some("Bearer invalid")] {
            let mut request = Request::builder().uri("/api/auth/me");
            if let Some(value) = authorization {
                request = request.header(header::AUTHORIZATION, value);
            }
            let response = router.clone().oneshot(request.body(Body::empty())?).await?;
            // 두 인증 실패 경로 모두 401과 Bearer 챌린지를 반환한다.
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer");
        }
        Ok(())
    }
}
