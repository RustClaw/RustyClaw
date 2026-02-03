use anyhow::{Context, Result};
use bollard::container::{CreateContainerOptions, Config};
use bollard::exec::CreateExecOptions;
use bollard::image::CreateImageOptions;
use bollard::Docker;
use std::collections::HashMap;
use tracing::{debug, info};
use futures::stream::StreamExt;

/// Docker client wrapper with RustyClaw-specific helpers
pub struct DockerClient {
    client: Docker,
}

/// Result of command execution in container
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i64,
}

/// Configuration for creating a sandbox container
pub struct ContainerConfig {
    pub image: String,
    pub workspace_mode: crate::sandbox::security::WorkspaceMode,
    pub workspace_path: String,
    pub network_enabled: bool,
    pub setup_command: Option<String>,
    pub env_vars: Vec<(String, String)>,
    pub labels: HashMap<String, String>,
}

impl DockerClient {
    /// Create a new Docker client
    pub async fn new() -> Result<Self> {
        let client = Docker::connect_with_local_defaults()
            .context("Failed to connect to Docker daemon")?;

        debug!("Connected to Docker daemon");
        Ok(Self { client })
    }

    /// Pull an image from registry if not present
    pub async fn pull_image(&self, image: &str) -> Result<()> {
        // First check if image exists
        if self.client.inspect_image(image).await.is_ok() {
            debug!("Image already exists: {}", image);
            return Ok(());
        }

        info!("Pulling Docker image: {}", image);

        let create_image_options = CreateImageOptions {
            from_image: image,
            ..Default::default()
        };

        let mut stream = self.client.create_image(Some(create_image_options), None, None);

        // Consume the stream to ensure the image is pulled
        while let Some(_) = stream.next().await {
            // Just consume the stream
        }

        info!("Successfully pulled Docker image: {}", image);
        Ok(())
    }

    /// Create a sandbox container with specified configuration
    pub async fn create_sandbox_container(
        &self,
        name: &str,
        config: &ContainerConfig,
    ) -> Result<String> {
        // Pull image first
        self.pull_image(&config.image).await?;

        // Prepare environment variables
        let mut env_vars: Vec<String> = config
            .env_vars
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        // Add default environment
        env_vars.push("PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string());

        // Prepare bind mounts for workspace
        let mut binds = vec![];
        if config.workspace_mode != crate::sandbox::security::WorkspaceMode::None {
            let mode = match config.workspace_mode {
                crate::sandbox::security::WorkspaceMode::ReadOnly => "ro",
                crate::sandbox::security::WorkspaceMode::ReadWrite => "rw",
                _ => "rw",
            };

            let container_path = match config.workspace_mode {
                crate::sandbox::security::WorkspaceMode::ReadOnly => "/agent",
                _ => "/workspace",
            };

            binds.push(format!(
                "{}:{}:{}",
                config.workspace_path, container_path, mode
            ));
        } else {
            // For isolated mode, create workspace dir in container at /workspace
            // User can mount isolated sandbox dir if needed
        }

        // Create container config
        let container_config = Config {
            image: Some(config.image.clone()),
            env: Some(env_vars),
            working_dir: Some("/workspace".to_string()),
            host_config: Some(bollard::models::HostConfig {
                binds: if !binds.is_empty() { Some(binds) } else { None },
                network_mode: if config.network_enabled {
                    Some("bridge".to_string())
                } else {
                    Some("none".to_string())
                },
                ..Default::default()
            }),
            labels: Some(config.labels.clone()),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name,
            platform: None,
        };

        let response = self
            .client
            .create_container(Some(options), container_config)
            .await
            .context("Failed to create container")?;

        info!("Created container: {} (id: {})", name, response.id);

        Ok(response.id)
    }

    /// Execute a command in a container and capture output
    pub async fn exec_command(
        &self,
        container_id: &str,
        command: &[&str],
    ) -> Result<ExecResult> {
        // Create exec instance
        let exec_options = CreateExecOptions {
            cmd: Some(command.to_vec()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let exec_id = self
            .client
            .create_exec(container_id, exec_options)
            .await
            .context("Failed to create exec instance")?;

        // Start exec and capture output
        use bollard::exec::StartExecResults;

        let output = self
            .client
            .start_exec(&exec_id.id, None)
            .await
            .context("Failed to start exec")?;

        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached { mut output, input: _ } = output {
            use futures::stream::StreamExt;
            while let Some(Ok(msg)) = output.next().await {
                match msg {
                    bollard::container::LogOutput::StdOut { message } => {
                        stdout.push_str(&String::from_utf8_lossy(&message));
                    }
                    bollard::container::LogOutput::StdErr { message } => {
                        stderr.push_str(&String::from_utf8_lossy(&message));
                    }
                    _ => {}
                }
            }
        }

        // Get exit code
        let inspect_result = self
            .client
            .inspect_exec(&exec_id.id)
            .await
            .context("Failed to inspect exec")?;

        let exit_code = inspect_result.exit_code.unwrap_or(-1) as i64;

        debug!(
            "Command executed in container: exit_code={}, stdout_len={}, stderr_len={}",
            exit_code,
            stdout.len(),
            stderr.len()
        );

        Ok(ExecResult {
            stdout,
            stderr,
            exit_code,
        })
    }

    /// List all containers with rustyclaw labels
    pub async fn list_sandbox_containers(&self) -> Result<Vec<ContainerInfo>> {
        use bollard::container::ListContainersOptions;

        let mut filters = HashMap::new();
        filters.insert("label".to_string(), vec!["rustyclaw.scope".to_string()]);

        let options = ListContainersOptions {
            all: true,
            filters,
            ..Default::default()
        };

        let containers = self
            .client
            .list_containers(Some(options))
            .await
            .context("Failed to list containers")?;

        let mut result = vec![];

        for container in containers {
            if let (Some(id), Some(name)) = (container.id, container.names.and_then(|mut n| n.pop())) {
                let name = name.trim_start_matches('/').to_string();
                result.push(ContainerInfo { id, name });
            }
        }

        Ok(result)
    }

    /// Check if a container exists and is running
    pub async fn container_exists(&self, container_id: &str) -> Result<bool> {
        let options = bollard::container::InspectContainerOptions {
            size: false,
        };
        match self.client.inspect_container(container_id, Some(options)).await {
            Ok(_) => Ok(true),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404,
                ..
            }) => Ok(false),
            Err(e) => Err(anyhow::anyhow!("Failed to inspect container: {}", e)),
        }
    }

    /// Remove a container
    pub async fn remove_container(&self, container_id: &str) -> Result<()> {
        // Stop container first if running
        let _ = self.client.stop_container(container_id, None).await;

        self.client
            .remove_container(container_id, None)
            .await
            .context("Failed to remove container")?;

        info!("Removed container: {}", container_id);
        Ok(())
    }

    /// Start a stopped container
    pub async fn start_container(&self, container_id: &str) -> Result<()> {
        let options: Option<bollard::container::StartContainerOptions<String>> = None;
        self.client
            .start_container(container_id, options)
            .await
            .context("Failed to start container")?;

        info!("Started container: {}", container_id);
        Ok(())
    }
}

/// Information about a sandbox container
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
}
