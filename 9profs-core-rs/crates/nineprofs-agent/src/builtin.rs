use crate::{AgentBackendDescriptor, AgentBackendKind};

#[derive(Clone, Debug)]
pub struct BuiltinAgentCatalog {
    descriptors: Vec<AgentBackendDescriptor>,
}

impl BuiltinAgentCatalog {
    pub fn load() -> Self {
        let mut descriptors = vec![
            AgentBackendDescriptor::builtin(
                "nineprofs-default",
                "9Profs Default",
                "Reserved 9Profs backend descriptor; execution is not enabled in Phase 2A.",
                AgentBackendKind::Embedded,
                ["cancellation"],
                0,
            ),
            AgentBackendDescriptor::builtin(
                "codex",
                "Codex",
                "Future Codex backend descriptor; no CLI or process is started in Phase 2A.",
                AgentBackendKind::Cli,
                ["streaming", "cancellation"],
                10,
            ),
            AgentBackendDescriptor::builtin(
                "claude",
                "Claude",
                "Future Claude backend descriptor; no CLI or process is started in Phase 2A.",
                AgentBackendKind::Cli,
                ["streaming", "cancellation"],
                20,
            ),
        ];
        descriptors.sort_by(|left, right| left.id.cmp(&right.id));
        Self { descriptors }
    }

    pub fn list(&self) -> &[AgentBackendDescriptor] {
        &self.descriptors
    }
}
