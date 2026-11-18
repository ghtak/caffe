pub mod entity;
pub mod repository;
pub mod service;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub refresh_token_ttl_seconds: u64,
    pub refresh_token_family_ttl_seconds: u64,
}

mod feature;
use std::sync::Arc;

pub use feature::user_create_local;

use crate::app::Container;

pub fn router() -> axum::Router<Arc<Container>> {
    axum::Router::new().route(
        "/user/local",
        axum::routing::post(feature::user_create_local::endpoint::handler),
    )
}
