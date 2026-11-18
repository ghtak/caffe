use std::sync::Arc;

use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};

use crate::app::Container;
use crate::app::auth::claims::{AuthClaims, UserClaims};

impl FromRequestParts<Arc<Container>> for UserClaims {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<Container>,
    ) -> Result<Self, Self::Rejection> {
        <Self as OptionalFromRequestParts<Arc<Container>>>::from_request_parts(parts, state)
            .await?
            .ok_or_else(|| bearer_rejection("Authentication required", "auth.extractor"))
    }
}

impl OptionalFromRequestParts<Arc<Container>> for UserClaims {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<Container>,
    ) -> Result<Option<Self>, Self::Rejection> {
        let claims = <AuthClaims as OptionalFromRequestParts<Arc<Container>>>::from_request_parts(
            parts, state,
        )
        .await?;
        claims
            .map(UserClaims::try_from)
            .transpose()
            .map_err(|_| bearer_rejection("Invalid access token", "auth.extractor"))
    }
}

impl FromRequestParts<Arc<Container>> for AuthClaims {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<Container>,
    ) -> Result<Self, Self::Rejection> {
        <Self as OptionalFromRequestParts<Arc<Container>>>::from_request_parts(parts, state)
            .await?
            .ok_or_else(|| bearer_rejection("Authentication required", "auth.extractor"))
    }
}

impl OptionalFromRequestParts<Arc<Container>> for AuthClaims {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<Container>,
    ) -> Result<Option<Self>, Self::Rejection> {
        extract_claims(&parts.headers, &state.jwt_token_service)
            .map(|claims| claims.map(Self))
            .map_err(|()| bearer_rejection("Invalid access token", "auth.extractor"))
    }
}

pub(super) fn bearer_rejection(message: &'static str, at: &'static str) -> Response {
    let mut response = crate::app::http::response::ApiResponse::from(
        crate::Error::new(crate::Code::Unauthorized, message).at(at),
    )
    .into_response();
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

fn extract_claims(
    headers: &HeaderMap,
    service: &espr::jwt::TokenService,
) -> Result<Option<espr::jwt::Claims>, ()> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(());
    }
    let value = value.to_str().map_err(|_| ())?;
    let (scheme, token) = value.split_once(' ').ok_or(())?;
    let token = token.trim_start_matches(' ');
    if !scheme.eq_ignore_ascii_case("Bearer")
        || token.is_empty()
        || token.bytes().any(|b| b.is_ascii_whitespace())
    {
        return Err(());
    }
    service.verify(token).map(Some).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::{HeaderMap, HeaderValue, header};

    use super::extract_claims;
    use crate::app::auth::claims::AuthClaims;

    #[test]
    fn optional_claims_can_be_used_as_a_handler_extractor() {
        async fn handler(_: Option<AuthClaims>) {}
        // Axum 핸들러에서 선택적 인증 추출기로 사용 가능하다.
        let _: axum::Router<Arc<crate::app::Container>> =
            axum::Router::new().route("/", axum::routing::get(handler));
    }

    #[test]
    fn optional_claims_only_accepts_valid_bearer_tokens() -> anyhow::Result<()> {
        let dir = std::env::temp_dir().join(format!("conbira-extractor-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir)?;
        let keys = espr::jwt::ed25519::Config {
            key_path: dir.join("private.pem"),
            public_key_path: dir.join("public.pem"),
        };
        espr::jwt::ed25519::generate(&keys)?;
        let service = espr::jwt::TokenService::new(&espr::jwt::Config {
            issuer: "test".into(),
            audience: "test".into(),
            access_token_ttl_seconds: 900,
            ed25519: keys,
        })?;
        std::fs::remove_dir_all(dir)?;
        let token = service.issue("user-1", Default::default())?.token;
        let mut headers = HeaderMap::new();
        // 헤더가 없는 요청은 익명 요청으로 처리한다.
        assert!(extract_claims(&headers, &service).unwrap().is_none());
        headers.insert(header::AUTHORIZATION, format!("bearer {token}").parse()?);
        // 유효한 토큰만 검증된 사용자 클레임을 반환한다.
        assert_eq!(
            extract_claims(&headers, &service).unwrap().unwrap().sub,
            "user-1"
        );
        for value in [
            "Basic abc",
            "Bearer",
            "Bearer ",
            "Bearer invalid",
            "Bearer a b",
        ] {
            headers.insert(header::AUTHORIZATION, HeaderValue::from_str(value)?);
            assert!(extract_claims(&headers, &service).is_err(), "{value}");
        }
        headers.insert(header::AUTHORIZATION, format!("Bearer {token}").parse()?);
        headers.append(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer invalid"),
        );
        // 중복 인증 헤더는 하나를 임의로 선택하지 않고 거부한다.
        assert!(extract_claims(&headers, &service).is_err());
        Ok(())
    }
}
