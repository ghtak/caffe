pub mod entity;
mod feature;
pub mod repository;
use std::sync::Arc;

use crate::app::Container;

pub fn router() -> axum::Router<Arc<Container>> {
    axum::Router::new()
        .route(
            "/",
            axum::routing::post(feature::sample_create::endpoint::handler)
                .get(feature::sample_search::endpoint::handler),
        )
        .route(
            "/{id}",
            axum::routing::put(feature::sample_update::endpoint::handler)
                .get(feature::sample_read::endpoint::handler),
        )
}
