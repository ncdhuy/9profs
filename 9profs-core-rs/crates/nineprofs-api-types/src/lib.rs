//! Transport DTOs shared by HTTP and WebSocket boundaries.
//!
//! This crate intentionally has no web-framework dependency.

mod response;
mod runtime;
mod websocket;

pub use response::{ApiResponse, ErrorResponse};
pub use runtime::{HealthResponse, RuntimeInfo};
pub use websocket::EventEnvelope;
