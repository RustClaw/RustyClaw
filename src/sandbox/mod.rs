mod docker;
mod container;
mod security;
mod workspace;
mod pruning;

pub use security::{SandboxMode, WorkspaceMode};
pub use docker::ExecResult;
pub use container::{ContainerMetadata, ContainerScope};
pub use pruning::PruningConfig;

use crate::config::SandboxConfig;
use anyhow::{Context, Result};
use container::ContainerManager;
use pruning::PruningService;
use security::SecurityPolicy;
use std::sync::Arc;
use tracing::info;

/// Main sandbox manager API
pub struct SandboxManager {
    container_manager: Arc<ContainerManager>,
    security_policy: SecurityPolicy,
    _pruning_service: Option<Arc<PruningService>>,
}

impl SandboxManager {
    /// Create a new sandbox manager
    pub async fn new(config: SandboxConfig) -> Result<Self> {
        let container_manager = Arc::new(ContainerManager::new(config.clone()).await?);
        let security_policy = SecurityPolicy {
            mode: config.mode.clone(),
        };

        // Start pruning service if enabled
        let pruning_service = if config.pruning.enabled {
            let service = Arc::new(PruningService::new(
                container_manager.clone(),
                config.pruning.clone(),
            ));

            let service_clone = service.clone();
            tokio::spawn(async move {
                service_clone.start().await;
            });

            info!("Sandbox pruning service started (idle_hours={}, max_age_days={})",
                config.pruning.idle_hours,
                config.pruning.max_age_days
            );

            Some(service)
        } else {
            info!("Sandbox pruning service disabled");
            None
        };

        info!(
            "Sandbox manager initialized: mode={:?}, scope={:?}, workspace={:?}",
            security_policy.mode, config.scope, config.workspace
        );

        Ok(Self {
            container_manager,
            security_policy,
            _pruning_service: pruning_service,
        })
    }

    /// Execute a command with sandboxing applied based on security policy
    pub async fn execute(
        &self,
        session_id: &str,
        is_main_session: bool,
        command: &[&str],
    ) -> Result<ExecResult> {
        if !self.security_policy.should_sandbox(is_main_session) {
            // Execute on host
            return self.execute_on_host(command).await;
        }

        // Execute in sandbox
        let container_id = self
            .container_manager
            .get_or_create_container(session_id)
            .await?;

        self.container_manager
            .execute_in_container(&container_id, command)
            .await
    }

    /// Execute a command directly on the host
    async fn execute_on_host(&self, command: &[&str]) -> Result<ExecResult> {
        use std::process::Command;

        let output = Command::new(command[0])
            .args(&command[1..])
            .output()
            .context("Failed to execute command on host")?;

        Ok(ExecResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1) as i64,
        })
    }

    /// List all active sandbox containers
    pub async fn list_containers(&self) -> Vec<ContainerMetadata> {
        self.container_manager.list_containers().await
    }

    /// Manually prune a specific container
    pub async fn prune_container(&self, scope_id: &str) -> Result<()> {
        self.container_manager.remove_container(scope_id).await
    }

    /// Get information about the sandbox configuration
    pub fn get_config_info(&self) -> String {
        format!(
            "Sandbox Configuration:\n\
            Mode: {:?}\n\
            Policy: {}\n\
            Sandboxing: {}",
            self.security_policy.mode,
            self.security_policy.describe(),
            if matches!(self.security_policy.mode, SandboxMode::Off) {
                "disabled"
            } else {
                "enabled"
            }
        )
    }
}
