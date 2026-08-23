//! Generic event publication and WebSocket transport foundation.

use std::sync::Arc;

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
};
use futures_util::StreamExt;
use nineprofs_api_types::EventEnvelope;
use tokio::sync::broadcast;

#[derive(Clone, Debug)]
pub struct BroadcastEventBus {
    sender: broadcast::Sender<EventEnvelope>,
}

impl BroadcastEventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope> {
        self.sender.subscribe()
    }

    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }

    pub fn publish(&self, event: EventEnvelope) -> Result<usize, EventEnvelope> {
        self.sender.send(event).map_err(|error| error.0)
    }
}

pub fn websocket_upgrade(ws: WebSocketUpgrade, bus: Arc<BroadcastEventBus>) -> Response {
    ws.on_upgrade(move |socket| serve_socket(socket, bus))
}

async fn serve_socket(mut socket: WebSocket, bus: Arc<BroadcastEventBus>) {
    let mut events = bus.subscribe();

    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok(event) => {
                    let Ok(payload) = serde_json::to_string(&event) else {
                        continue;
                    };
                    if socket.send(Message::Text(payload.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            message = socket.next() => match message {
                Some(Ok(Message::Ping(payload))) => {
                    if socket.send(Message::Pong(payload)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(Message::Text(_))) | Some(Ok(Message::Binary(_))) | Some(Ok(Message::Pong(_))) => {}
                Some(Err(_)) => break,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn event_bus_publishes_to_multiple_subscribers() {
        let bus = BroadcastEventBus::new(8);
        let mut first = bus.subscribe();
        let mut second = bus.subscribe();

        let event = EventEnvelope::new("runtime.started", json!({"ready": true}));
        assert_eq!(bus.publish(event.clone()).unwrap(), 2);

        assert_eq!(first.recv().await.unwrap(), event);
        assert_eq!(second.recv().await.unwrap(), event);
    }
}
