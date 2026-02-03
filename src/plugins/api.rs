use crate::plugins::traits::{HookType, PluginApi, PluginHook, Tool, ToolFactory};
use crate::Config;
use anyhow::Result;
use std::sync::Arc;
use tracing::debug;

/// Default implementation of PluginApi
pub struct DefaultPluginApi {
    config: Arc<Config>,
    tool_registry: Arc<crate::plugins::ToolRegistry>,
    _hook_runner: Arc<crate::plugins::HookRunner>,
}

impl DefaultPluginApi {
    /// Create a new plugin API
    pub fn new(
        config: Arc<Config>,
        tool_registry: Arc<crate::plugins::ToolRegistry>,
        hook_runner: Arc<crate::plugins::HookRunner>,
    ) -> Self {
        Self {
            config,
            tool_registry,
            _hook_runner: hook_runner,
        }
    }
}

impl PluginApi for DefaultPluginApi {
    fn register_tool(&self, tool: Tool) -> Result<()> {
        debug!("Registering tool: {}", tool.name);

        // Store tool in registry
        self.tool_registry.register_tool(tool)?;

        Ok(())
    }

    fn register_tool_factory(&self, factory: ToolFactory) -> Result<()> {
        debug!("Registering tool factory");

        // Store factory in registry
        self.tool_registry.register_tool_factory(factory)?;

        Ok(())
    }

    fn register_hook(&self, hook_type: HookType, _hook: PluginHook) -> Result<()> {
        debug!("Registering hook: {:?}", hook_type);

        // Note: We need to spawn this on a runtime
        // This will be handled by the plugin loader
        // For now, hook registration is deferred

        Ok(())
    }

    fn get_config(&self) -> Arc<Config> {
        self.config.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_creation() {
        // This would require mock objects
        // Placeholder for actual test when mocks are available
    }
}
