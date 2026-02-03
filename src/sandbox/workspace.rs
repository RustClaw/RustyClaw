use crate::sandbox::security::WorkspaceMode;
use anyhow::{Context, Result};

/// Workspace path information
#[allow(dead_code)]
pub struct WorkspacePaths {
    pub host_path: String,
    pub container_path: String,
    pub mode: &'static str,
}

/// Manages workspace mounting and isolation
#[allow(dead_code)]
pub struct WorkspaceManager {
    workspace_mode: WorkspaceMode,
}

impl WorkspaceManager {
    /// Create a new workspace manager
    #[allow(dead_code)]
    pub fn new(workspace_mode: WorkspaceMode) -> Self {
        Self { workspace_mode }
    }

    /// Prepare workspace paths for a container
    #[allow(dead_code)]
    pub fn prepare_workspace(&self, scope_id: &str) -> Result<WorkspacePaths> {
        match self.workspace_mode {
            WorkspaceMode::None => {
                // Create isolated sandbox dir
                let path = self.get_isolated_workspace(scope_id)?;
                std::fs::create_dir_all(&path)
                    .context("Failed to create isolated workspace directory")?;

                Ok(WorkspacePaths {
                    host_path: path,
                    container_path: "/workspace".to_string(),
                    mode: "rw",
                })
            }
            WorkspaceMode::ReadOnly => {
                let path = self.get_agent_workspace()?;
                std::fs::create_dir_all(&path)
                    .context("Failed to create agent workspace directory")?;

                Ok(WorkspacePaths {
                    host_path: path,
                    container_path: "/agent".to_string(),
                    mode: "ro",
                })
            }
            WorkspaceMode::ReadWrite => {
                let path = self.get_agent_workspace()?;
                std::fs::create_dir_all(&path)
                    .context("Failed to create agent workspace directory")?;

                Ok(WorkspacePaths {
                    host_path: path,
                    container_path: "/workspace".to_string(),
                    mode: "rw",
                })
            }
        }
    }

    /// Get the isolated workspace path for a session
    #[allow(dead_code)]
    fn get_isolated_workspace(&self, scope_id: &str) -> Result<String> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home
            .join(".rustyclaw")
            .join("sandboxes")
            .join(scope_id)
            .to_string_lossy()
            .to_string())
    }

    /// Get the agent workspace path
    #[allow(dead_code)]
    fn get_agent_workspace(&self) -> Result<String> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home
            .join(".rustyclaw")
            .join("workspace")
            .to_string_lossy()
            .to_string())
    }
}
