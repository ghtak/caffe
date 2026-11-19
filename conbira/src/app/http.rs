use std::sync::Arc;

use axum::http::StatusCode;
use axum::routing::get;
use espr::validate::Validate;
use tower_http::trace::TraceLayer;

use crate::app::Container;
use crate::error::ResultExt as _;

pub mod extract;
pub mod response;

pub fn router(container: Arc<Container>) -> crate::Result<axum::Router> {
    let router = axum::Router::new()
        .route("/api/health", get(|| async { StatusCode::OK }))
        .nest("/api/samples", super::sample::router())
        .nest("/api/rbac", super::rbac::router())
        .nest("/api/auth", super::auth::router());

    let router = match &container.config.http.static_files {
        Some(static_files) => {
            static_files
                .validate()
                .map_err_internal("Failed to validate static files", crate::here!())?;
            router.merge(static_files.as_router())
        }
        None => router,
    };
    let router =
        router.layer(TraceLayer::new_for_http().make_span_with(espr::http::make_request_span));

    Ok(router.with_state(container))
}
