/// Plugin trait definitions and core types
pub mod traits;

/// Hook system for lifecycle events
pub mod hooks;

/// Plugin API that plugins use
pub mod api;

/// Tool and plugin registries
pub mod registry;

// Re-export core types
pub use traits::{
    HookType, HookModification, PluginApi, PluginHook, PluginManifest, RustyclawPlugin,
    Tool, ToolContext, ToolFactory, ToolParameter, ToolResult, BeforeAgentStartEvent,
    BeforeToolCallEvent, AfterToolCallEvent, MessageReceivedEvent, MessageSendingEvent,
};

pub use hooks::HookRunner;
pub use api::DefaultPluginApi;
pub use registry::{PluginRegistry, ToolRegistry};

use anyhow::Result;
use std::sync::Arc;
use tracing::info;

/// Global plugin registry
static PLUGIN_REGISTRY: once_cell::sync::OnceCell<Arc<PluginRegistry>> =
    once_cell::sync::OnceCell::new();

/// Initialize the global plugin registry
pub fn init_plugin_registry() -> Arc<PluginRegistry> {
    PLUGIN_REGISTRY.get_or_init(|| Arc::new(PluginRegistry::new())).clone()
}

/// Get the global plugin registry
pub fn get_plugin_registry() -> Option<Arc<PluginRegistry>> {
    PLUGIN_REGISTRY.get().cloned()
}

/// Initialize plugins from a list
pub async fn initialize_plugins(
    plugins: Vec<Arc<dyn RustyclawPlugin>>,
    api: Arc<dyn PluginApi>,
) -> Result<Arc<PluginRegistry>> {
    let registry = init_plugin_registry();
    let plugin_count = plugins.len();

    info!("Initializing {} plugins", plugin_count);

    for plugin in plugins {
        let id = plugin.id().to_string();
        let name = plugin.name().to_string();
        let version = plugin.version().to_string();

        // Register plugin in registry
        registry
            .register_plugin(id.clone(), name.clone(), version.clone())
            .await?;

        // Call plugin's register function
        plugin.register(api.as_ref()).await?;

        // Call plugin's on_load hook
        plugin.on_load().await?;

        info!("✅ Loaded plugin: {} v{}", name, version);
    }

    info!("✅ {} plugins initialized", plugin_count);

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_registry_init() {
        let registry = init_plugin_registry();
        assert_eq!(
            get_plugin_registry().unwrap().plugin_count().await,
            0
        );
    }
}
