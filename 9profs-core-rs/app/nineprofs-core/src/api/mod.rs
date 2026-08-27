use std::sync::Arc;

use axum::Router;
use nineprofs_runtime::CoreRuntime;

pub(super) mod agents;
pub(super) mod assistants;
pub(super) mod documents;
mod error;
pub(super) mod mcp;
pub(super) mod officecli;
pub(super) mod proposals;
pub(super) mod research;
pub(super) mod skills;
pub(super) mod system;
pub(super) mod websocket;

#[cfg(test)]
#[allow(unused_imports)]
mod tests;

pub(crate) use error::ApiError;

#[derive(Clone)]
pub(crate) struct AppState {
    runtime: Arc<CoreRuntime>,
}

pub fn build_router(runtime: Arc<CoreRuntime>) -> Router {
    let state = AppState { runtime };
    Router::new()
        .merge(system::router())
        .merge(documents::router())
        .merge(proposals::router())
        .merge(officecli::router())
        .merge(agents::router())
        .merge(assistants::router())
        .merge(skills::router())
        .merge(mcp::router())
        .merge(research::router())
        .merge(websocket::router())
        .with_state(state)
}
