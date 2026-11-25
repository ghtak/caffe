pub mod claims;
pub mod entity;
pub mod extractor;
pub mod repository;
pub mod service;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub refresh_token: service::refresh_token::Config,
    pub password_hash: service::password_hash::Config,
    pub google_oidc: Option<service::google_oidc::Config>,
}

mod feature;
use std::sync::Arc;

pub use feature::user_create_local;

use crate::app::Container;

pub fn router() -> axum::Router<Arc<Container>> {
    axum::Router::new()
        .route(
            "/web/login/local",
            axum::routing::post(feature::web_session::login_local)
                .layer(axum::middleware::map_response(no_store)),
        )
        .route(
            "/web/login/google/complete",
            axum::routing::post(feature::web_session::login_google)
                .layer(axum::middleware::map_response(no_store)),
        )
        .route(
            "/web/refresh",
            axum::routing::post(feature::web_session::refresh)
                .layer(axum::middleware::map_response(no_store)),
        )
        .route(
            "/web/logout",
            axum::routing::post(feature::web_session::logout)
                .layer(axum::middleware::map_response(no_store)),
        )
        .route("/me", axum::routing::get(feature::me::handler))
        .route(
            "/refresh",
            axum::routing::post(feature::user_token_refresh::endpoint::handler)
                .layer(axum::middleware::map_response(no_store)),
        )
        .route(
            "/logout",
            axum::routing::post(feature::user_logout::endpoint::handler)
                .layer(axum::middleware::map_response(no_store)),
        )
        .route(
            "/users",
            axum::routing::get(feature::user_search::endpoint::handler),
        )
        .route(
            "/users/local",
            axum::routing::post(feature::user_create_local::endpoint::handler),
        )
        .route(
            "/users/{id}/status",
            axum::routing::patch(feature::user_status_update::endpoint::handler),
        )
        .route(
            "/login/local",
            axum::routing::post(feature::user_login_local::endpoint::handler),
        )
        .route(
            "/login/google",
            axum::routing::post(feature::user_login_google::endpoint::handler)
                .layer(axum::middleware::map_response(no_store)),
        )
        .route(
            "/login/google/complete",
            axum::routing::post(feature::user_login_google_complete::endpoint::handler)
                .layer(axum::middleware::map_response(no_store)),
        )
        .layer(tower_cookies::CookieManagerLayer::new())
}

async fn no_store(mut response: axum::response::Response) -> axum::response::Response {
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}
