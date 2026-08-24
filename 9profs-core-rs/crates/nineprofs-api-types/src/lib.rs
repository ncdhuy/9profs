//! Transport DTOs shared by HTTP and WebSocket boundaries.
//!
//! This crate intentionally has no web-framework dependency.

mod agent_run;
mod assistant;
mod mcp;
mod response;
mod runtime;
mod skill;
mod websocket;

pub use agent_run::{
    AgentRunDto, AgentRunRequest, AgentRunStartedDto, AgentTaskDto, AgentTaskFailureDto,
};
pub use assistant::{AssistantDto, CreateAssistantRequest, UpdateAssistantRequest};
pub use mcp::{
    CreateMcpServerRequest, McpConnectionTestDto, McpServerDto, McpToolDto, McpTransportDto,
    McpTransportInputDto, UpdateMcpServerRequest,
};
pub use response::{ApiResponse, ErrorResponse};
pub use runtime::{HealthResponse, RuntimeInfo};
pub use skill::{SkillCatalogDto, SkillDto, SkillIssueDto};
pub use websocket::EventEnvelope;
