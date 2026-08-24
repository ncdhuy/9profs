use std::sync::Arc;

use nineprofs_db::Database;
use nineprofs_mcp::{
    CreateMcpServer, McpService, McpTransportConfig, SqliteMcpServerRepository, stable_tool_id,
};
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_tools::{ToolError, ToolId, ToolInvocation, ToolRegistry, ToolSet, ToolSource};
use serde_json::json;

fn fixture_path() -> String {
    env!("CARGO_BIN_EXE_mcp_fixture").to_owned()
}

fn fixture_server(enabled: bool) -> CreateMcpServer {
    CreateMcpServer {
        id: Some("fixture".to_owned()),
        name: "Fixture MCP".to_owned(),
        description: "deterministic test server".to_owned(),
        enabled,
        startup_timeout_ms: Some(5_000),
        transport: McpTransportConfig::Stdio {
            command: fixture_path(),
            args: Vec::new(),
            env: [("MCP_FIXTURE_SECRET".to_owned(), "never-log-me".to_owned())]
                .into_iter()
                .collect(),
        },
    }
}

fn fixture_server_as(id: &str, enabled: bool) -> CreateMcpServer {
    let mut server = fixture_server(enabled);
    server.id = Some(id.to_owned());
    server.name = format!("Fixture MCP {id}");
    server
}

#[test]
fn stable_tool_ids_escape_raw_names_without_collisions() {
    let server = nineprofs_mcp::McpServerId::new("server").unwrap();
    assert_eq!(stable_tool_id(&server, "echo"), "mcp/server/echo");
    assert_ne!(
        stable_tool_id(&server, "a/b"),
        stable_tool_id(&server, "a%2Fb")
    );
}

#[tokio::test]
async fn stdio_mcp_tools_cross_the_9profs_runtime_boundary() {
    let registry = ToolRegistry::new();
    let database = Database::in_memory().await.unwrap();
    let service = McpService::new(
        SqliteMcpServerRepository::new(database.pool().clone()),
        registry.clone(),
        Arc::new(BroadcastEventBus::new(32)),
    );
    service.create(fixture_server(true)).await.unwrap();
    let tested = service.test("fixture").await.unwrap();
    assert!(tested.success);
    assert_eq!(tested.tool_count, 1);
    assert_eq!(
        service.get("fixture").await.unwrap().status,
        nineprofs_mcp::McpServerStatus::Disconnected
    );
    assert!(registry.list_definitions().is_empty());

    let server = service.connect("fixture").await.unwrap();
    assert_eq!(server.status, nineprofs_mcp::McpServerStatus::Connected);
    let tool = &server.tools[0];
    assert_eq!(tool.name, "echo");
    assert_eq!(
        tool.id,
        stable_tool_id(&nineprofs_mcp::McpServerId::new("fixture").unwrap(), "echo")
    );
    assert_eq!(registry.list_definitions()[0].source, ToolSource::Mcp);

    let invocation = ToolInvocation::new(ToolId::new(tool.id.clone()), json!({"value": 9}));
    assert!(matches!(
        registry
            .execute(invocation.clone(), &ToolSet::default())
            .await,
        Err(ToolError::ToolNotAuthorized(_))
    ));
    let result = registry
        .execute(invocation, &ToolSet::from_ids([tool.id.clone()]))
        .await
        .unwrap();
    assert_eq!(result.output, json!(r#"{"value":9}"#));

    service.disconnect("fixture").await.unwrap();
    assert!(registry.list_definitions().is_empty());
}

#[tokio::test]
async fn persisted_secrets_are_redacted_from_snapshots_and_debug() {
    let database = Database::in_memory().await.unwrap();
    let service = McpService::new(
        SqliteMcpServerRepository::new(database.pool().clone()),
        ToolRegistry::new(),
        Arc::new(BroadcastEventBus::new(32)),
    );
    service.create(fixture_server(false)).await.unwrap();
    let snapshot = service.get("fixture").await.unwrap();
    let encoded = serde_json::to_string(&snapshot).unwrap();
    assert!(!encoded.contains("never-log-me"));
    assert!(encoded.contains("MCP_FIXTURE_SECRET"));
    assert!(!format!("{snapshot:?}").contains("never-log-me"));
}

#[tokio::test]
async fn duplicate_raw_names_are_namespaced_and_refresh_is_atomic() {
    let registry = ToolRegistry::new();
    let database = Database::in_memory().await.unwrap();
    let service = McpService::new(
        SqliteMcpServerRepository::new(database.pool().clone()),
        registry.clone(),
        Arc::new(BroadcastEventBus::new(32)),
    );
    service
        .create(fixture_server_as("first", true))
        .await
        .unwrap();
    service
        .create(fixture_server_as("second", true))
        .await
        .unwrap();
    service.connect("first").await.unwrap();
    service.connect("second").await.unwrap();
    let definitions = registry.list_definitions();
    assert_eq!(definitions.len(), 2);
    assert_ne!(definitions[0].id, definitions[1].id);
    assert_ne!(definitions[0].name, definitions[1].name);

    service.set_enabled("first", false).await.unwrap();
    let definitions = registry.list_definitions();
    assert_eq!(definitions.len(), 1);
    assert!(definitions[0].id.as_str().starts_with("mcp/second/"));
    service.delete("second").await.unwrap();
    assert!(registry.list_definitions().is_empty());
}

#[tokio::test]
async fn failed_server_does_not_corrupt_other_connections_and_timeout_is_bounded() {
    let registry = ToolRegistry::new();
    let database = Database::in_memory().await.unwrap();
    let service = McpService::new(
        SqliteMcpServerRepository::new(database.pool().clone()),
        registry.clone(),
        Arc::new(BroadcastEventBus::new(32)),
    );
    service
        .create(fixture_server_as("healthy", true))
        .await
        .unwrap();
    let mut failed = fixture_server_as("failed", true);
    if let McpTransportConfig::Stdio { command, .. } = &mut failed.transport {
        *command = "definitely-not-a-real-mcp-command".to_owned();
    }
    service.create(failed).await.unwrap();
    assert!(service.connect("failed").await.is_err());
    assert_eq!(
        service.get("failed").await.unwrap().status,
        nineprofs_mcp::McpServerStatus::Error
    );
    service.connect("healthy").await.unwrap();
    assert_eq!(registry.list_definitions().len(), 1);

    let mut delayed = fixture_server_as("delayed", true);
    delayed.startup_timeout_ms = Some(10);
    if let McpTransportConfig::Stdio { env, .. } = &mut delayed.transport {
        env.insert("MCP_FIXTURE_DELAY_MS".to_owned(), "100".to_owned());
    }
    service.create(delayed).await.unwrap();
    let result = service.test("delayed").await.unwrap();
    assert!(!result.success);
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("timed out")
    );
    assert_eq!(
        service.get("healthy").await.unwrap().status,
        nineprofs_mcp::McpServerStatus::Connected
    );
}

#[tokio::test]
async fn connection_failure_removes_stale_tools_from_registry() {
    let registry = ToolRegistry::new();
    let database = Database::in_memory().await.unwrap();
    let service = McpService::new(
        SqliteMcpServerRepository::new(database.pool().clone()),
        registry.clone(),
        Arc::new(BroadcastEventBus::new(32)),
    );
    let mut server = fixture_server(true);
    if let McpTransportConfig::Stdio { env, .. } = &mut server.transport {
        env.insert("MCP_FIXTURE_FAIL_CALL".to_owned(), "1".to_owned());
    }
    service.create(server).await.unwrap();
    let connected = service.connect("fixture").await.unwrap();
    let tool_id = connected.tools[0].id.clone();

    let result = registry
        .execute(
            ToolInvocation::new(ToolId::new(tool_id), json!({})),
            &ToolSet::from_ids([connected.tools[0].id.clone()]),
        )
        .await;
    assert!(matches!(result, Err(ToolError::Handler(_))));
    assert!(registry.list_definitions().is_empty());
    assert_eq!(
        service.get("fixture").await.unwrap().status,
        nineprofs_mcp::McpServerStatus::Error
    );
}
