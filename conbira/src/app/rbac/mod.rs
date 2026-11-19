pub mod entity;
mod feature;
pub mod repository;

pub fn router() -> axum::Router<std::sync::Arc<crate::app::Container>> {
    axum::Router::new()
        .route(
            "/roles",
            axum::routing::get(feature::role_list::endpoint::handler)
                .post(feature::role_create::endpoint::handler),
        )
        .route(
            "/roles/{id}",
            axum::routing::delete(feature::role_delete::endpoint::handler),
        )
        .route(
            "/permissions",
            axum::routing::get(feature::permission_list::endpoint::handler)
                .post(feature::permission_create::endpoint::handler),
        )
        .route(
            "/permissions/{id}",
            axum::routing::delete(feature::permission_delete::endpoint::handler),
        )
}
