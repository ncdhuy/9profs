use axum::Router;

use super::AppState;

pub(super) mod cases;
pub(super) mod citation_expectation;
pub(super) mod citation_review;
pub(super) mod citations;
pub(super) mod claim_coverage;
pub(super) mod claims;
pub(super) mod common;
pub(super) mod cross_claim_assessment;
pub(super) mod cross_claim_candidates;
pub(super) mod evidence;
pub(super) mod manuscript;
pub(super) mod manuscript_research_review;
pub(super) mod pdf;
pub(super) mod reference_resolution;
pub(super) mod retrieval;
pub(super) mod sources;
pub(super) mod verification;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .merge(cases::router())
        .merge(claim_coverage::router())
        .merge(cross_claim_candidates::router())
        .merge(cross_claim_assessment::router())
        .merge(citation_review::router())
        .merge(citation_expectation::router())
        .merge(sources::router())
        .merge(pdf::router())
        .merge(evidence::router())
        .merge(claims::router())
        .merge(citations::router())
        .merge(manuscript::router())
        .merge(manuscript_research_review::router())
        .merge(verification::router())
        .merge(retrieval::router())
        .merge(reference_resolution::router())
}
