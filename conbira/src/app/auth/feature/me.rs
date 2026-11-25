use axum::response::Response;

use crate::app::auth::claims::AuthClaims;
use crate::app::http::response::ApiResponse;

pub async fn handler(claims: AuthClaims) -> Result<ApiResponse<espr::jwt::Claims>, Response> {
    Ok(claims.0.into())
}

#[cfg(test)]
mod tests {
    mod contract {
        use axum::body::to_bytes;
        use axum::http::StatusCode;
        use axum::response::IntoResponse;

        use super::super::*;

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
            let response = handler(AuthClaims(claims)).await.unwrap().into_response();
            let status = response.status();
            let body: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
            // 응답 데이터는 추출된 클레임 전체와 일치한다.
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["data"], expected);
            Ok(())
        }
    }
}
