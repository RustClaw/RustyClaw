//! Dynamic system prompt builder
//!
//! Assembles the system prompt from workspace files and runtime context,
//! following the OpenClaw-style approach.

use crate::config::workspace::{Workspace, WorkspaceFile};
use crate::llm::ToolDefinition;
use chrono::{Local, Utc};
use std::env;

/// Builds dynamic system prompts from workspace files and runtime context
pub struct SystemPromptBuilder {
    workspace: Workspace,
    tools: Vec<ToolDefinition>,
}

impl SystemPromptBuilder {
    /// Create a new prompt builder
    pub fn new(workspace: Workspace, tools: Vec<ToolDefinition>) -> Self {
        Self { workspace, tools }
    }

    /// Build the complete system prompt
    pub fn build(&self) -> String {
        let mut sections = Vec::new();

        // 1. Identity and Soul
        if let Some(section) = self.build_identity_section() {
            sections.push(section);
        }

        // 2. Tooling information
        sections.push(self.build_tooling_section());

        // 3. Safety guardrails
        sections.push(self.build_safety_section());

        // 4. Operating instructions (AGENTS.md)
        if let Some(section) = self.build_agents_section() {
            sections.push(section);
        }

        // 5. User preferences (USER.md)
        if let Some(section) = self.build_user_section() {
            sections.push(section);
        }

        // 6. Runtime information
        sections.push(self.build_runtime_section());

        // 7. Memory Context (Daily Log + Curated)
        if let Some(section) = self.build_memory_section() {
            sections.push(section);
        }

        // 8. Current time
        sections.push(self.build_time_section());

        sections.join("\n\n")
    }

    /// Build identity section from IDENTITY.md and SOUL.md
    fn build_identity_section(&self) -> Option<String> {
        let mut parts = Vec::new();

        if let Some(identity) = self.workspace.load_file(WorkspaceFile::Identity) {
            parts.push(identity);
        }

        if let Some(soul) = self.workspace.load_file(WorkspaceFile::Soul) {
            parts.push(soul);
        }

        if parts.is_empty() {
            // Fallback identity
            Some("You are RustyClaw, a helpful AI assistant.".to_string())
        } else {
            Some(parts.join("\n\n"))
        }
    }

    /// Build tooling section with available tools and creation guide
    fn build_tooling_section(&self) -> String {
        let mut section = String::from("## Available Tools\n\n");

        if self.tools.is_empty() {
            section.push_str("No tools are currently available.\n");
        } else {
            section.push_str("You have access to the following tools:\n\n");
            for tool in &self.tools {
                section.push_str(&format!("- `{}`: {}\n", tool.name, tool.description));
            }
        }

        // Add TOOLS.md content for tool creation instructions
        if let Some(tools_guide) = self.workspace.load_file(WorkspaceFile::Tools) {
            section.push_str("\n\n");
            section.push_str(&tools_guide);
        }

        section
    }

    /// Build safety guardrails section
    fn build_safety_section(&self) -> String {
        String::from(
            "## Safety Guidelines\n\n\
             - Always prioritize user safety and privacy\n\
             - Never bypass oversight mechanisms or safety controls\n\
             - Be transparent about your actions and limitations\n\
             - Ask for clarification when instructions are ambiguous",
        )
    }

    /// Build operating instructions from AGENTS.md
    fn build_agents_section(&self) -> Option<String> {
        self.workspace.load_file(WorkspaceFile::Agents)
    }

    /// Build user preferences from USER.md
    fn build_user_section(&self) -> Option<String> {
        self.workspace.load_file(WorkspaceFile::User)
    }

    /// Build runtime information section
    fn build_runtime_section(&self) -> String {
        let os = env::consts::OS;
        let arch = env::consts::ARCH;

        format!(
            "## Runtime\n\n\
             - **Platform**: {} ({})\n\
             - **Gateway**: RustyClaw",
            os, arch
        )
    }

    /// Build memory context section
    fn build_memory_section(&self) -> Option<String> {
        use crate::core::memory::MemoryManager;
        // Create memory manager on the fly since it's just a path wrapper
        let memory_manager = MemoryManager::new(self.workspace.path());

        let mut parts = Vec::new();

        // Add curated memory
        if let Some(curated) = memory_manager.get_curated_memory() {
            parts.push(format!("## Long-Term Memory\n\n{}", curated));
        }

        // Add daily log
        if let Ok(today) = memory_manager.get_today_log() {
            if !today.trim().is_empty() {
                parts.push(format!("## Recent Memory (Today)\n{}", today));
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n\n"))
        }
    }

    /// Build current time section
    fn build_time_section(&self) -> String {
        let utc_time = Utc::now();
        let local_time = Local::now();

        format!(
            "## Current Time\n\n\
             - **UTC**: {}\n\
             - **Local**: {}",
            utc_time.format("%Y-%m-%d %H:%M:%S UTC"),
            local_time.format("%Y-%m-%d %H:%M:%S %Z")
        )
    }
}

/// Build a system prompt with minimal context (for sub-agents)
pub fn build_minimal_prompt(tools: &[ToolDefinition]) -> String {
    let mut prompt = String::from("You are a helpful AI assistant.\n\n");

    if !tools.is_empty() {
        prompt.push_str("## Available Tools\n\n");
        for tool in tools {
            prompt.push_str(&format!("- `{}`: {}\n", tool.name, tool.description));
        }
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_prompt_builder_creates_prompt() {
        let dir = tempdir().unwrap();
        let workspace = Workspace::new(dir.path().join("workspace"));
        workspace.init_default().unwrap();

        let builder = SystemPromptBuilder::new(workspace, vec![]);
        let prompt = builder.build();

        // Should contain key sections
        assert!(prompt.contains("RustyClaw") || prompt.contains("Identity"));
        assert!(prompt.contains("## Available Tools"));
        assert!(prompt.contains("## Safety Guidelines"));
        assert!(prompt.contains("## Runtime"));
        assert!(prompt.contains("## Current Time"));
    }

    #[test]
    fn test_prompt_builder_includes_tools() {
        let dir = tempdir().unwrap();
        let workspace = Workspace::new(dir.path().join("workspace"));
        workspace.init_default().unwrap();

        let tools = vec![ToolDefinition {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            parameters: serde_json::json!({}),
        }];

        let builder = SystemPromptBuilder::new(workspace, tools);
        let prompt = builder.build();

        assert!(prompt.contains("`test_tool`"));
        assert!(prompt.contains("A test tool"));
    }

    #[test]
    fn test_minimal_prompt() {
        let prompt = build_minimal_prompt(&[]);
        assert!(prompt.contains("helpful AI assistant"));
    }
}
