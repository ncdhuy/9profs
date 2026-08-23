//! Small, framework-independent primitives shared by 9Profs Core crates.

mod error;
mod id;
mod timestamp;

pub use error::{CoreError, CoreResult};
pub use id::new_id;
pub use timestamp::{TimestampMs, now_ms};
