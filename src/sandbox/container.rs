use crate::config::SandboxConfig;
use crate::sandbox::docker::{DockerClient, ExecResult};
use crate::sandbox::security::WorkspaceMode;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Scope for container lifecycle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ContainerScope {
    /// One container per session
    Session,
    /// One container per agent
    Agent,
    /// Single shared container for all
    Shared,
}

/// Metadata about a sandbox container
#[derive(Debug, Clone)]
pub struct ContainerMetadata {
    pub id: String,
    pub name: String,
    pub scope: ContainerScope,
    pub scope_id: String,
    pub created_at: DateTime<Utc>,
    pub last_used: DateTime<Utc>,
    pub image: String,
}

/// Manages container lifecycle and caching
pub struct ContainerManager {
    docker: Arc<DockerClient>,
    containers: Arc<RwLock<HashMap<String, ContainerMetadata>>>,
    config: SandboxConfig,
}

impl ContainerManager {
    /// Create a new container manager
    pub async fn new(config: SandboxConfig) -> Result<Self> {
        let docker = Arc::new(DockerClient::new().await?);

        // Discover existing containers from Docker
        let containers = Self::discover_existing_containers(&docker).await?;

        info!(
            "Container manager initialized with {} existing sandbox containers",
            containers.len()
        );

        Ok(Self {
            docker,
            containers: Arc::new(RwLock::new(containers)),
            config,
        })
    }

    /// Get the Docker client
    pub fn get_docker(&self) -> Arc<DockerClient> {
        self.docker.clone()
    }

    /// Get or create a container for the given scope
    pub async fn get_or_create_container(&self, scope_id: &str) -> Result<String> {
        // Check cache first
        {
            let containers = self.containers.read().await;
            if let Some(meta) = containers.get(scope_id) {
                // Verify container still exists
                if self.docker.container_exists(&meta.id).await? {
                    // Update last_used
                    self.update_last_used(scope_id).await;
                    debug!("Reusing existing container for scope: {}", scope_id);
                    return Ok(meta.id.clone());
                } else {
                    debug!(
                        "Container for scope {} no longer exists, will recreate",
                        scope_id
                    );
                }
            }
        }

        // Create new container
        let container_id = self.create_container(scope_id).await?;

        // Cache it
        {
            let mut containers = self.containers.write().await;
            containers.insert(
                scope_id.to_string(),
                ContainerMetadata {
                    id: container_id.clone(),
                    name: format!("rustyclaw-sandbox-{}", scope_id),
                    scope: self.config.scope.clone(),
                    scope_id: scope_id.to_string(),
                    created_at: Utc::now(),
                    last_used: Utc::now(),
                    image: self.config.image.clone(),
                },
            );
        }

        Ok(container_id)
    }

    /// Create a new sandbox container
    async fn create_container(&self, scope_id: &str) -> Result<String> {
        let container_name = format!("rustyclaw-sandbox-{}", scope_id);

        // Prepare workspace
        let workspace_path = match self.config.workspace {
            WorkspaceMode::None => {
                // Create isolated dir
                let path = self.get_sandbox_workspace_path(scope_id)?;
                std::fs::create_dir_all(&path)
                    .context("Failed to create sandbox workspace directory")?;
                path
            }
            WorkspaceMode::ReadOnly | WorkspaceMode::ReadWrite => {
                // Use agent workspace
                let path = self.get_agent_workspace_path()?;
                std::fs::create_dir_all(&path)
                    .context("Failed to create agent workspace directory")?;
                path
            }
        };

        let config = crate::sandbox::docker::ContainerConfig {
            image: self.config.image.clone(),
            workspace_mode: self.config.workspace.clone(),
            workspace_path,
            network_enabled: self.config.network,
            setup_command: self.config.setup_command.clone(),
            env_vars: vec![],
            labels: HashMap::from([
                (
                    "rustyclaw.scope".to_string(),
                    format!("{:?}", self.config.scope),
                ),
                (
                    "rustyclaw.scope_id".to_string(),
                    scope_id.to_string(),
                ),
                (
                    "rustyclaw.created_at".to_string(),
                    Utc::now().to_rfc3339(),
                ),
            ]),
        };

        let container_id = self
            .docker
            .create_sandbox_container(&container_name, &config)
            .await?;

        // Start the container
        self.docker.start_container(&container_id).await?;

        // Run setup command if specified
        if let Some(setup_cmd) = &self.config.setup_command {
            info!("Running setup command in container: {}", setup_cmd);
            let result = self
                .docker
                .exec_command(&container_id, &["sh", "-c", setup_cmd])
                .await?;

            if result.exit_code != 0 {
                return Err(anyhow::anyhow!(
                    "Setup command failed with exit code {}: {}",
                    result.exit_code,
                    result.stderr
                ));
            }
        }

        info!(
            "Created and configured sandbox container: {} (id: {})",
            container_name, container_id
        );

        Ok(container_id)
    }

    /// Get the workspace path for an isolated sandbox
    fn get_sandbox_workspace_path(&self, scope_id: &str) -> Result<String> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home
            .join(".rustyclaw")
            .join("sandboxes")
            .join(scope_id)
            .to_string_lossy()
            .to_string())
    }

    /// Get the agent workspace path
    fn get_agent_workspace_path(&self) -> Result<String> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home
            .join(".rustyclaw")
            .join("workspace")
            .to_string_lossy()
            .to_string())
    }

    /// Update the last_used timestamp for a container
    async fn update_last_used(&self, scope_id: &str) {
        let mut containers = self.containers.write().await;
        if let Some(meta) = containers.get_mut(scope_id) {
            meta.last_used = Utc::now();
        }
    }

    /// List all cached containers
    pub async fn list_containers(&self) -> Vec<ContainerMetadata> {
        let containers = self.containers.read().await;
        containers.values().cloned().collect()
    }

    /// Remove a container
    pub async fn remove_container(&self, scope_id: &str) -> Result<()> {
        let container_id = {
            let containers = self.containers.read().await;
            containers.get(scope_id).map(|m| m.id.clone())
        };

        if let Some(id) = container_id {
            self.docker.remove_container(&id).await?;
        }

        let mut containers = self.containers.write().await;
        containers.remove(scope_id);

        info!("Removed sandbox container for scope: {}", scope_id);
        Ok(())
    }

    /// Execute a command in a container
    pub async fn execute_in_container(
        &self,
        container_id: &str,
        command: &[&str],
    ) -> Result<ExecResult> {
        self.docker.exec_command(container_id, command).await
    }

    /// Discover existing containers with rustyclaw labels
    async fn discover_existing_containers(
        docker: &Arc<DockerClient>,
    ) -> Result<HashMap<String, ContainerMetadata>> {
        let sandbox_containers = docker.list_sandbox_containers().await?;
        let mut result = HashMap::new();

        for container in sandbox_containers {
            // Try to extract scope_id from container name
            // Name format: rustyclaw-sandbox-{scope_id}
            if let Some(scope_id_str) = container.name.strip_prefix("rustyclaw-sandbox-") {
                let scope_id = scope_id_str.to_string();
                result.insert(
                    scope_id.clone(),
                    ContainerMetadata {
                        id: container.id,
                        name: container.name,
                        scope: ContainerScope::Session, // Default, could be improved
                        scope_id,
                        created_at: Utc::now(), // Could be improved by reading from labels
                        last_used: Utc::now(),
                        image: "unknown".to_string(), // Could be improved
                    },
                );
            }
        }

        Ok(result)
    }
}
