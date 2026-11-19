pub mod claims;
pub mod entity;
pub mod extractor;
pub mod repository;
pub mod service;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub refresh_token: service::refresh_token::Config,
    pub password_hash: service::password_hash::Config,
}

mod feature;
use std::sync::Arc;

pub use feature::user_create_local;

use crate::app::Container;

pub fn router() -> axum::Router<Arc<Container>> {
    axum::Router::new()
        .route("/me", axum::routing::get(feature::auth_me::handler))
        .route(
            "/users/local",
            axum::routing::post(feature::user_create_local::endpoint::handler),
        )
        .route(
            "/login/local",
            axum::routing::post(feature::user_login_local::endpoint::handler),
        )
}
