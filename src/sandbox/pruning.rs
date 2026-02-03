use crate::sandbox::container::ContainerManager;
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::info;

/// Configuration for container pruning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruningConfig {
    /// Enable automatic pruning
    #[serde(default = "default_pruning_enabled")]
    pub enabled: bool,

    /// Remove containers idle for this many hours
    #[serde(default = "default_idle_hours")]
    pub idle_hours: u64,

    /// Remove containers older than this many days
    #[serde(default = "default_max_age_days")]
    pub max_age_days: u64,

    /// Check interval in minutes
    #[serde(default = "default_check_interval_minutes")]
    pub check_interval_minutes: u64,
}

impl Default for PruningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            idle_hours: 24,
            max_age_days: 7,
            check_interval_minutes: 60,
        }
    }
}

fn default_pruning_enabled() -> bool {
    true
}

fn default_idle_hours() -> u64 {
    24
}

fn default_max_age_days() -> u64 {
    7
}

fn default_check_interval_minutes() -> u64 {
    60
}

/// Service for automatic cleanup of idle containers
pub struct PruningService {
    manager: Arc<ContainerManager>,
    config: PruningConfig,
}

impl PruningService {
    /// Create a new pruning service
    pub fn new(manager: Arc<ContainerManager>, config: PruningConfig) -> Self {
        Self { manager, config }
    }

    /// Start the pruning service (runs in background)
    pub async fn start(&self) {
        let mut interval = interval(Duration::from_secs(self.config.check_interval_minutes * 60));

        loop {
            interval.tick().await;

            if let Err(e) = self.prune_containers().await {
                tracing::error!("Failed to prune containers: {}", e);
            }
        }
    }

    /// Check and prune containers according to policy
    pub async fn prune_containers(&self) -> Result<()> {
        let containers = self.manager.list_containers().await;
        let now = Utc::now();

        let mut pruned_count = 0;

        for container in containers {
            let idle_duration = now - container.last_used;
            let age_duration = now - container.created_at;

            let idle_hours = idle_duration.num_hours();
            let age_days = age_duration.num_days();

            let should_remove = idle_hours >= self.config.idle_hours as i64
                || age_days >= self.config.max_age_days as i64;

            if should_remove {
                info!(
                    "Pruning sandbox container {} (idle: {}h, age: {}d, config: idle_limit={}h, max_age={}d)",
                    container.scope_id, idle_hours, age_days,
                    self.config.idle_hours, self.config.max_age_days
                );

                if let Err(e) = self.manager.remove_container(&container.scope_id).await {
                    tracing::warn!("Failed to prune container {}: {}", container.scope_id, e);
                } else {
                    pruned_count += 1;
                }
            }
        }

        if pruned_count > 0 {
            info!("Pruned {} idle sandbox containers", pruned_count);
        }

        Ok(())
    }
}
