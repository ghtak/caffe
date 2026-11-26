//! File metadata persistence, configuration, and object storage.
//! HTTP file access and attachment changes; cleanup belongs to later tasks.

pub mod access;
pub mod config;
pub mod entity;
mod feature;
pub mod repository;
pub mod s3;
pub use config::Config;
pub use feature::file_group_attach::usecase as file_group_attach;

pub fn router() -> axum::Router<std::sync::Arc<crate::app::Container>> {
    axum::Router::new()
        .route(
            "/files",
            axum::routing::get(feature::file_list::endpoint::handler)
                .post(feature::file_upload::endpoint::handler)
                .layer(axum::extract::DefaultBodyLimit::disable()),
        )
        .route(
            "/file-groups/{group_id}/files",
            axum::routing::get(feature::file_group_list::endpoint::handler),
        )
        .route(
            "/file-groups/{group_id}/attach",
            axum::routing::post(feature::file_group_attach::endpoint::handler),
        )
        .route(
            "/files/{file_id}/download",
            axum::routing::get(feature::file_download::endpoint::handler),
        )
        .route(
            "/files/{file_id}",
            axum::routing::delete(feature::file_delete::endpoint::handler),
        )
        .layer(axum::middleware::map_response(no_store))
}

async fn no_store(mut response: axum::response::Response) -> axum::response::Response {
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "tests/http.rs"]
mod tests;
