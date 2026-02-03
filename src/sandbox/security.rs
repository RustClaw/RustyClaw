use serde::{Deserialize, Serialize};

/// Sandbox execution modes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxMode {
    /// No sandboxing, direct host execution
    Off,
    /// Only non-main sessions sandboxed
    NonMain,
    /// All sessions sandboxed
    All,
}

impl Default for SandboxMode {
    fn default() -> Self {
        SandboxMode::NonMain
    }
}

/// Workspace access modes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    /// Isolated workspace per container
    None,
    /// Read-only agent workspace mount
    ReadOnly,
    /// Read-write agent workspace mount
    ReadWrite,
}

impl Default for WorkspaceMode {
    fn default() -> Self {
        WorkspaceMode::None
    }
}

/// Security policy for sandbox execution
pub struct SecurityPolicy {
    pub mode: SandboxMode,
}

impl SecurityPolicy {
    /// Check if a session should be sandboxed
    pub fn should_sandbox(&self, is_main_session: bool) -> bool {
        match self.mode {
            SandboxMode::Off => false,
            SandboxMode::NonMain => !is_main_session,
            SandboxMode::All => true,
        }
    }

    /// Get a human-readable description of the mode
    pub fn describe(&self) -> &'static str {
        match self.mode {
            SandboxMode::Off => "Sandboxing disabled - all code runs on host",
            SandboxMode::NonMain => {
                "Non-main sessions run in sandbox, main session runs on host"
            }
            SandboxMode::All => "All sessions run in sandbox",
        }
    }
}
