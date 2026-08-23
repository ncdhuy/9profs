use std::sync::Arc;

use axum::{
    Router,
    extract::{State, WebSocketUpgrade},
    response::Response,
    routing::get,
};
use nineprofs_api_types::{ApiResponse, HealthResponse, RuntimeInfo};
use nineprofs_runtime::CoreRuntime;

#[derive(Clone)]
struct AppState {
    runtime: Arc<CoreRuntime>,
}

pub fn build_router(runtime: Arc<CoreRuntime>) -> Router {
    let state = AppState { runtime };
    Router::new()
        .route("/api/health", get(health))
        .route("/api/runtime", get(runtime_info))
        .route("/ws", get(websocket))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> axum::Json<ApiResponse<HealthResponse>> {
    axum::Json(ApiResponse::ok(state.runtime.health()))
}

async fn runtime_info(State(state): State<AppState>) -> axum::Json<ApiResponse<RuntimeInfo>> {
    axum::Json(ApiResponse::ok(state.runtime.info()))
}

async fn websocket(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    nineprofs_realtime::websocket_upgrade(upgrade, state.runtime.event_bus())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use futures_util::SinkExt;
    use tokio::net::TcpListener;
    use tokio_tungstenite::connect_async;
    use tower::ServiceExt;

    async fn test_router() -> Router {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        build_router(runtime)
    }

    #[tokio::test]
    async fn health_endpoint_returns_stable_payload() {
        let response = test_router()
            .await
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["status"], "ok");
        assert_eq!(json["data"]["service"], "9profs-core");
    }

    #[tokio::test]
    async fn websocket_endpoint_accepts_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, test_router().await).await.unwrap();
        });

        let (mut socket, _) = connect_async(format!("ws://{address}/ws")).await.unwrap();
        socket.close(None).await.unwrap();
        server.abort();
    }
}
