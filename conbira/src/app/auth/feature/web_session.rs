use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use tower_cookies::cookie::SameSite;
use tower_cookies::cookie::time::Duration;
use tower_cookies::{Cookie, Cookies};

use super::{user_login_google_complete, user_login_local, user_logout, user_token_refresh};
use crate::app::Container;
use crate::app::auth::service::login_tokens::LoginTokens;
use crate::app::http::extract::ApiJson;
use crate::app::http::response::ApiResponse;
use crate::{Code, Error};

const COOKIE: &str = "__Host-conbira-refresh";

type WebResult = Result<Response, ApiResponse<()>>;

#[derive(serde::Serialize)]
struct WebTokens {
    access_token: String,
    expires_at: i64,
    claims: crate::app::auth::claims::UserClaims,
}

fn check_request(headers: &HeaderMap) -> crate::Result<()> {
    if headers
        .get("x-conbira-request")
        .and_then(|v| v.to_str().ok())
        != Some("1")
    {
        return Err(Error::new(Code::Forbidden, "Invalid browser request").at(crate::here!()));
    }
    Ok(())
}

fn token(cookies: &Cookies) -> crate::Result<String> {
    cookies
        .get(COOKIE)
        .map(|cookie| cookie.value().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::new(Code::Unauthorized, "Invalid refresh token").at(crate::here!()))
}

fn session_cookie(value: String, ttl: u64) -> Cookie<'static> {
    Cookie::build((COOKIE, value))
        .path("/")
        .secure(true)
        .http_only(true)
        .same_site(SameSite::Strict)
        .max_age(Duration::seconds(ttl.min(i64::MAX as u64) as i64))
        .build()
}

fn token_response(cookies: &Cookies, tokens: LoginTokens, ttl: u64) -> WebResult {
    cookies.add(session_cookie(tokens.refresh_token, ttl));
    Ok(ApiResponse::from(WebTokens {
        access_token: tokens.access_token,
        expires_at: tokens.expires_at,
        claims: tokens.claims,
    })
    .into_response())
}

pub async fn login_local(
    State(container): State<Arc<Container>>,
    headers: HeaderMap,
    cookies: Cookies,
    ApiJson(request): ApiJson<user_login_local::endpoint::RequestBody>,
) -> WebResult {
    check_request(&headers)?;
    let tokens = user_login_local::usecase::execute(
        &container.db,
        &container.password_hash_service,
        &container.dummy_password_hash,
        &container.jwt_token_service,
        &container.refresh_token_service,
        user_login_local::usecase::Command {
            email: request.email,
            password: request.password,
        },
    )
    .await?;
    token_response(
        &cookies,
        tokens,
        container.config.auth.refresh_token.ttl_seconds,
    )
}

pub async fn login_google(
    State(container): State<Arc<Container>>,
    headers: HeaderMap,
    cookies: Cookies,
    ApiJson(request): ApiJson<user_login_google_complete::endpoint::RequestBody>,
) -> WebResult {
    check_request(&headers)?;
    let oidc = container
        .google_oidc_service
        .as_ref()
        .ok_or_else(|| Error::new(Code::NotFound, "Google login is disabled").at(crate::here!()))?;
    let tokens = user_login_google_complete::usecase::execute(
        &container,
        oidc,
        request.code,
        request.state,
        request.browser_proof,
    )
    .await?;
    token_response(
        &cookies,
        tokens,
        container.config.auth.refresh_token.ttl_seconds,
    )
}

pub async fn refresh(
    State(container): State<Arc<Container>>,
    headers: HeaderMap,
    cookies: Cookies,
) -> WebResult {
    check_request(&headers)?;
    let tokens = user_token_refresh::usecase::execute(
        &container.db,
        &container.jwt_token_service,
        &container.refresh_token_service,
        &token(&cookies)?,
    )
    .await?;
    token_response(
        &cookies,
        tokens,
        container.config.auth.refresh_token.ttl_seconds,
    )
}

pub async fn logout(
    State(container): State<Arc<Container>>,
    headers: HeaderMap,
    cookies: Cookies,
) -> WebResult {
    check_request(&headers)?;
    if let Ok(token) = token(&cookies) {
        user_logout::usecase::execute(&container.db, &container.refresh_token_service, &token)
            .await?;
    }
    cookies.remove(session_cookie(String::new(), 0));
    Ok(ApiResponse::empty().into_response())
}
