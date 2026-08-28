use axum::Router;

use super::AppState;

pub(super) mod cases;
pub(super) mod citations;
pub(super) mod claims;
pub(super) mod common;
pub(super) mod evidence;
pub(super) mod manuscript;
pub(super) mod pdf;
pub(super) mod reference_resolution;
pub(super) mod retrieval;
pub(super) mod sources;
pub(super) mod verification;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .merge(cases::router())
        .merge(sources::router())
        .merge(pdf::router())
        .merge(evidence::router())
        .merge(claims::router())
        .merge(citations::router())
        .merge(manuscript::router())
        .merge(verification::router())
        .merge(retrieval::router())
        .merge(reference_resolution::router())
}
