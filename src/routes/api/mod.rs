//! The HTTP API, for scripts and monitoring tools: JSON in and out, a key in
//! the `Authorization` header as the only proof. It sits beside the pages,
//! outside their CSRF check, signed-in user lookup and error pages, and no
//! handler here reads the session, so a cookie opens nothing.

mod caller;
mod error;
mod services;

use std::any::Any;

use axum::Router;
use axum::http::header::CACHE_CONTROL;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware;
use axum::routing::get;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;

use super::{REQUEST_TIMEOUT, not_found, panic_response};
use crate::middleware::rate_limit::RateLimit;
use crate::state::AppState;

/// A request carries a few fields of JSON.
const BODY_LIMIT: usize = 64 * 1024;

/// The API routes, to nest under `/api`. Unknown addresses below it answer
/// in JSON too. The rate limit is the one of the pages: one budget per
/// address.
pub(super) fn api_routes(rate_limit: &RateLimit) -> Router<AppState> {
    Router::new()
        .route("/v1/services", get(services::list))
        .fallback(not_found)
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(CatchPanicLayer::custom(|panic: Box<dyn Any + Send>| {
            panic_response(&*panic)
        }))
        .layer(rate_limit.layer())
        .layer(middleware::from_fn(error::json_errors))
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
}
