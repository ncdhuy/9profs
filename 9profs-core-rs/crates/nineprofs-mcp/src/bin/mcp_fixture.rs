use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout());
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(method) = request.get("method").and_then(Value::as_str) else {
            continue;
        };
        let Some(id) = request.get("id") else {
            continue;
        };
        if method == "initialize"
            && let Some(delay_ms) = std::env::var("MCP_FIXTURE_DELAY_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
        {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
        let call_failed =
            method == "tools/call" && std::env::var("MCP_FIXTURE_FAIL_CALL").as_deref() == Ok("1");
        let result = match method {
            "initialize" => json!({
                "protocolVersion": "2025-03-26",
                "capabilities": {"tools": {}, "resources": {}},
                "serverInfo": {"name": "nineprofs-fixture", "version": "1.0"}
            }),
            "tools/list" => json!({
                "tools": [{
                    "name": "echo",
                    "description": "Echo supplied JSON",
                    "inputSchema": {"type": "object"}
                }]
            }),
            "tools/call" => {
                let arguments = request
                    .get("params")
                    .and_then(|params| params.get("arguments"))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                json!({
                    "content": [{"type": "text", "text": serde_json::to_string(&arguments).unwrap()}],
                    "isError": false
                })
            }
            _ => continue,
        };
        let response = if call_failed {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32000, "message": "fixture call failure"}
            })
        } else {
            json!({"jsonrpc": "2.0", "id": id, "result": result})
        };
        writeln!(stdout, "{}", response).unwrap();
        stdout.flush().unwrap();
    }
}
