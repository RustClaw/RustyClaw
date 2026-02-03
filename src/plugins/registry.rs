use crate::plugins::traits::{Tool, ToolContext, ToolFactory};
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

/// Manages tool registration and tool factory invocation
pub struct ToolRegistry {
    /// Static tools that don't change (using Mutex for sync access)
    static_tools: Arc<Mutex<HashMap<String, Tool>>>,

    /// Tool factories that create context-aware tools
    tool_factories: Arc<Mutex<Vec<Arc<ToolFactory>>>>,
}

impl ToolRegistry {
    /// Create a new tool registry
    pub fn new() -> Self {
        Self {
            static_tools: Arc::new(Mutex::new(HashMap::new())),
            tool_factories: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register a static tool
    pub fn register_tool(&self, tool: Tool) -> Result<()> {
        let name = tool.name.clone();
        debug!("Registering static tool: {}", name);

        let mut tools = self
            .static_tools
            .lock()
            .map_err(|e| anyhow!("Failed to acquire tool registry lock: {}", e))?;

        if tools.contains_key(&name) {
            warn!("Tool '{}' already registered, overwriting", name);
        }

        tools.insert(name, tool);
        Ok(())
    }

    /// Register a tool factory for context-aware tools
    pub fn register_tool_factory(&self, factory: ToolFactory) -> Result<()> {
        debug!("Registering tool factory");

        let mut factories = self
            .tool_factories
            .lock()
            .map_err(|e| anyhow!("Failed to acquire factory registry lock: {}", e))?;

        factories.push(Arc::new(factory));
        Ok(())
    }

    /// Get all available tools for a session
    pub async fn get_tools(&self, ctx: &ToolContext) -> Result<Vec<Tool>> {
        let mut tools = Vec::new();

        // Add static tools
        let static_tools = self
            .static_tools
            .lock()
            .map_err(|e| anyhow!("Failed to acquire tool registry lock: {}", e))?;

        for tool in static_tools.values() {
            tools.push(Tool {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.parameters.clone(),
                execute: tool.execute.clone(),
            });
        }

        // Factory-generated tools would be created here
        // (requires async context which isn't available during registration)

        debug!("Retrieved {} tools for session {}", tools.len(), ctx.session_id);
        Ok(tools)
    }

    /// Get a specific tool by name
    pub fn get_tool(&self, name: &str) -> Result<Option<Tool>> {
        let tools = self
            .static_tools
            .lock()
            .map_err(|e| anyhow!("Failed to acquire tool registry lock: {}", e))?;
        Ok(tools.get(name).cloned())
    }

    /// List all tool names
    pub fn list_tools(&self) -> Result<Vec<String>> {
        let tools = self
            .static_tools
            .lock()
            .map_err(|e| anyhow!("Failed to acquire tool registry lock: {}", e))?;
        let names: Vec<String> = tools.keys().cloned().collect();
        Ok(names)
    }

    /// Clear all tools (useful for testing)
    pub fn clear_all(&self) {
        let mut tools = self.static_tools.lock().ok();
        if let Some(ref mut t) = tools {
            t.clear();
        }

        let mut factories = self.tool_factories.lock().ok();
        if let Some(ref mut f) = factories {
            f.clear();
        }

        info!("Tool registry cleared");
    }

    /// Get tool count
    pub fn tool_count(&self) -> usize {
        self.static_tools
            .lock()
            .map(|tools| tools.len())
            .unwrap_or(0)
    }

    /// Get tool factory count
    pub fn factory_count(&self) -> usize {
        self.tool_factories
            .lock()
            .map(|factories| factories.len())
            .unwrap_or(0)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ToolRegistry {
    fn clone(&self) -> Self {
        Self {
            static_tools: self.static_tools.clone(),
            tool_factories: self.tool_factories.clone(),
        }
    }
}

/// Plugin registry - manages all loaded plugins
pub struct PluginRegistry {
    /// Map of plugin ID -> plugin metadata
    plugins: Arc<Mutex<HashMap<String, PluginEntry>>>,

    /// Tool registry
    pub tools: Arc<ToolRegistry>,

    /// Hook runner
    pub hooks: Arc<crate::plugins::HookRunner>,
}

/// Entry for a registered plugin
struct PluginEntry {
    id: String,
    name: String,
    version: String,
    enabled: bool,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(Mutex::new(HashMap::new())),
            tools: Arc::new(ToolRegistry::new()),
            hooks: Arc::new(crate::plugins::HookRunner::new()),
        }
    }

    /// Register a plugin
    pub async fn register_plugin(
        &self,
        id: String,
        name: String,
        version: String,
    ) -> Result<()> {
        let mut plugins = self
            .plugins
            .lock()
            .map_err(|e| anyhow!("Failed to acquire plugin registry lock: {}", e))?;

        if plugins.contains_key(&id) {
            return Err(anyhow!("Plugin '{}' already registered", id));
        }

        plugins.insert(
            id.clone(),
            PluginEntry {
                id: id.clone(),
                name,
                version,
                enabled: true,
            },
        );

        info!("Registered plugin: {}", id);
        Ok(())
    }

    /// Unregister a plugin
    pub async fn unregister_plugin(&self, id: &str) -> Result<()> {
        let mut plugins = self
            .plugins
            .lock()
            .map_err(|e| anyhow!("Failed to acquire plugin registry lock: {}", e))?;

        if !plugins.contains_key(id) {
            return Err(anyhow!("Plugin '{}' not found", id));
        }

        plugins.remove(id);
        info!("Unregistered plugin: {}", id);
        Ok(())
    }

    /// Check if plugin is enabled
    pub async fn is_plugin_enabled(&self, id: &str) -> Result<bool> {
        let plugins = self
            .plugins
            .lock()
            .map_err(|e| anyhow!("Failed to acquire plugin registry lock: {}", e))?;
        Ok(plugins
            .get(id)
            .map(|p| p.enabled)
            .ok_or_else(|| anyhow!("Plugin '{}' not found", id))?)
    }

    /// Enable plugin
    pub async fn enable_plugin(&self, id: &str) -> Result<()> {
        let mut plugins = self
            .plugins
            .lock()
            .map_err(|e| anyhow!("Failed to acquire plugin registry lock: {}", e))?;
        let plugin = plugins
            .get_mut(id)
            .ok_or_else(|| anyhow!("Plugin '{}' not found", id))?;
        plugin.enabled = true;
        info!("Enabled plugin: {}", id);
        Ok(())
    }

    /// Disable plugin
    pub async fn disable_plugin(&self, id: &str) -> Result<()> {
        let mut plugins = self
            .plugins
            .lock()
            .map_err(|e| anyhow!("Failed to acquire plugin registry lock: {}", e))?;
        let plugin = plugins
            .get_mut(id)
            .ok_or_else(|| anyhow!("Plugin '{}' not found", id))?;
        plugin.enabled = false;
        info!("Disabled plugin: {}", id);
        Ok(())
    }

    /// List all plugins
    pub async fn list_plugins(&self) -> Result<Vec<(String, String, String, bool)>> {
        let plugins = self
            .plugins
            .lock()
            .map_err(|e| anyhow!("Failed to acquire plugin registry lock: {}", e))?;
        let list = plugins
            .values()
            .map(|p| (p.id.clone(), p.name.clone(), p.version.clone(), p.enabled))
            .collect();
        Ok(list)
    }

    /// Get plugin count
    pub async fn plugin_count(&self) -> usize {
        self.plugins
            .lock()
            .map(|plugins| plugins.len())
            .unwrap_or(0)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for PluginRegistry {
    fn clone(&self) -> Self {
        Self {
            plugins: self.plugins.clone(),
            tools: self.tools.clone(),
            hooks: self.hooks.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_tool(name: &str) -> Tool {
        Tool {
            name: name.to_string(),
            description: "Test tool".to_string(),
            parameters: json!({"type": "object", "properties": {}}),
            execute: Arc::new(|_| {
                Box::pin(async {
                    Ok(crate::plugins::traits::ToolResult {
                        content: "Test".to_string(),
                        details: None,
                        success: true,
                    })
                })
            }),
        }
    }

    #[test]
    fn test_tool_registry_creation() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.tool_count(), 0);
    }

    #[test]
    fn test_register_tool() {
        let registry = ToolRegistry::new();
        let tool = create_test_tool("test_tool");

        registry.register_tool(tool).unwrap();
        assert_eq!(registry.tool_count(), 1);
    }

    #[test]
    fn test_list_tools() {
        let registry = ToolRegistry::new();
        registry.register_tool(create_test_tool("tool1")).unwrap();
        registry.register_tool(create_test_tool("tool2")).unwrap();

        let names = registry.list_tools().unwrap();
        assert_eq!(names.len(), 2);
    }

    #[tokio::test]
    async fn test_plugin_registry_creation() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.plugin_count().await, 0);
    }

    #[tokio::test]
    async fn test_register_plugin() {
        let registry = PluginRegistry::new();
        registry
            .register_plugin("test-plugin".to_string(), "Test".to_string(), "1.0.0".to_string())
            .await
            .unwrap();

        assert_eq!(registry.plugin_count().await, 1);
        assert!(registry.is_plugin_enabled("test-plugin").await.unwrap());
    }

    #[tokio::test]
    async fn test_plugin_enable_disable() {
        let registry = PluginRegistry::new();
        registry
            .register_plugin("test-plugin".to_string(), "Test".to_string(), "1.0.0".to_string())
            .await
            .unwrap();

        registry.disable_plugin("test-plugin").await.unwrap();
        assert!(!registry.is_plugin_enabled("test-plugin").await.unwrap());

        registry.enable_plugin("test-plugin").await.unwrap();
        assert!(registry.is_plugin_enabled("test-plugin").await.unwrap());
    }
}
