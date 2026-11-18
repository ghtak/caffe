pub mod entity;
pub mod repository;

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
