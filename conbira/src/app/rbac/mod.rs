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
            "/roles/{role_id}/permissions/{permission_id}",
            axum::routing::post(feature::role_permission_add::endpoint::handler)
                .delete(feature::role_permission_remove::endpoint::handler),
        )
        .route(
            "/users/{user_id}/roles",
            axum::routing::get(feature::user_role_list::endpoint::handler),
        )
        .route(
            "/users/{user_id}/roles/{role_id}",
            axum::routing::post(feature::user_role_add::endpoint::handler)
                .delete(feature::user_role_remove::endpoint::handler),
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
