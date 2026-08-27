use crate::api::AppState;
use axum::Router;
use axum::extract::State;
use axum::extract::WebSocketUpgrade;
use axum::response::Response;
use axum::routing::get;

async fn websocket(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    nineprofs_realtime::websocket_upgrade(upgrade, state.runtime.event_bus())
}

async fn document_websocket(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    nineprofs_documents::websocket_upgrade(upgrade, state.runtime.document_bridge())
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/ws", get(websocket))
        .route("/ws/documents", get(document_websocket))
}
