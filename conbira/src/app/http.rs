use std::sync::Arc;

use axum::routing::get;
use espr::validate::Validate;
use tower_http::trace::TraceLayer;

use crate::Error;
use crate::app::Container;

pub mod extract;
pub mod response;

pub fn router(container: Arc<Container>) -> crate::Result<axum::Router> {
    let router = axum::Router::new().route("/api/health", get(|| async { "ok" }));

    let router = router.nest("/api/sample", super::sample::router());

    let router = match &container.config.http.static_files {
        Some(static_files) => {
            static_files.validate().map_err(|e| {
                Error::internal("static files validation failed")
                    .at("http.router.static_files")
                    .cause(e)
            })?;
            router.merge(static_files.as_router())
        }
        None => router,
    };
    let router =
        router.layer(TraceLayer::new_for_http().make_span_with(espr::http::make_request_span));

    Ok(router.with_state(container))
}
