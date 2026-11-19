pub mod entity;
mod feature;
pub mod repository;

pub fn router() -> axum::Router<std::sync::Arc<crate::app::Container>> {
    axum::Router::new().route(
        "/permissions",
        axum::routing::get(feature::permission_list::endpoint::handler),
    )
}
