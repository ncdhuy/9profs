use std::sync::Arc;

use nineprofs_core::build_router;
use nineprofs_runtime::{CoreRuntime, RuntimeConfig};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = RuntimeConfig::from_env();
    let bind_addr = config.bind_addr;
    let runtime = Arc::new(CoreRuntime::initialize(config).await?);
    let listener = TcpListener::bind(bind_addr).await?;

    println!("9Profs Core listening on http://{bind_addr}");
    axum::serve(listener, build_router(runtime)).await?;
    Ok(())
}
