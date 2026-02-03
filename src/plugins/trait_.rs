use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Context provided to tools during execution
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Session identifier
    pub session_id: String,

    /// Workspace directory (if applicable)
    pub workspace_dir: Option<String>,

    /// Agent ID (if applicable)
    pub agent_id: Option<String>,

    /// Message channel (if applicable)
    pub message_channel: Option<String>,

    /// Whether code is sandboxed
    pub sandboxed: bool,

    /// Additional context data
    pub metadata: HashMap<String, Value>,
}

/// Tool parameter definition using JSON Schema
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub description: String,
    pub schema: Value,
    #[serde(default)]
    pub required: bool,
}

/// Tool execution result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolResult {
    /// Main content of the result
    pub content: String,

    /// Additional details (optional)
    pub details: Option<HashMap<String, Value>>,

    /// Whether execution was successful
    pub success: bool,
}

/// Tool definition - what the LLM sees
#[derive(Clone)]
pub struct Tool {
    /// Unique tool name
    pub name: String,

    /// Human-readable description
    pub description: String,

    /// Parameter schema (JSON Schema)
    pub parameters: Value,

    /// Tool executor function
    pub execute: Arc<
        dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<ToolResult>> + Send>> + Send + Sync,
    >,
}

/// Type alias for tool factories (context-aware tool creators)
pub type ToolFactory = Box<dyn Fn(ToolContext) -> Tool + Send + Sync>;

/// Core plugin trait that all plugins must implement
pub trait RustyclawPlugin: Send + Sync {
    /// Plugin unique identifier
    fn id(&self) -> &str;

    /// Plugin display name
    fn name(&self) -> &str;

    /// Plugin version
    fn version(&self) -> &str;

    /// Plugin description
    fn description(&self) -> &str {
        ""
    }

    /// Called when plugin is registered
    /// Plugins register tools and hooks here
    fn register(&self, api: &dyn PluginApi) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

    /// Called when plugin loads (after registration)
    fn on_load(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    /// Called when plugin unloads
    fn on_unload(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    /// Get plugin configuration schema
    fn config_schema(&self) -> Option<Value> {
        None
    }
}

/// Plugin API - what plugins can do
pub trait PluginApi: Send + Sync {
    /// Register a static tool
    fn register_tool(&self, tool: Tool) -> Result<()>;

    /// Register a context-aware tool factory
    fn register_tool_factory(&self, factory: ToolFactory) -> Result<()>;

    /// Register a hook
    fn register_hook(&self, hook_type: HookType, hook: PluginHook) -> Result<()>;

    /// Get global configuration
    fn get_config(&self) -> Arc<crate::Config>;

    /// Get storage backend
    fn get_storage(&self) -> Arc<dyn crate::storage::Storage>;
}

/// All available hook types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookType {
    /// Before agent starts processing a message
    BeforeAgentStart,

    /// After agent finishes
    AgentEnd,

    /// Before context compaction
    BeforeCompaction,

    /// After context compaction
    AfterCompaction,

    /// When a user message arrives
    MessageReceived,

    /// Before sending a message
    MessageSending,

    /// After message is sent
    MessageSent,

    /// Before tool is called
    BeforeToolCall,

    /// After tool is called
    AfterToolCall,

    /// Before tool result is persisted
    ToolResultPersist,

    /// Session starts
    SessionStart,

    /// Session ends
    SessionEnd,

    /// Gateway starts
    GatewayStart,

    /// Gateway stops
    GatewayStop,
}

impl std::fmt::Display for HookType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HookType::BeforeAgentStart => write!(f, "before_agent_start"),
            HookType::AgentEnd => write!(f, "agent_end"),
            HookType::BeforeCompaction => write!(f, "before_compaction"),
            HookType::AfterCompaction => write!(f, "after_compaction"),
            HookType::MessageReceived => write!(f, "message_received"),
            HookType::MessageSending => write!(f, "message_sending"),
            HookType::MessageSent => write!(f, "message_sent"),
            HookType::BeforeToolCall => write!(f, "before_tool_call"),
            HookType::AfterToolCall => write!(f, "after_tool_call"),
            HookType::ToolResultPersist => write!(f, "tool_result_persist"),
            HookType::SessionStart => write!(f, "session_start"),
            HookType::SessionEnd => write!(f, "session_end"),
            HookType::GatewayStart => write!(f, "gateway_start"),
            HookType::GatewayStop => write!(f, "gateway_stop"),
        }
    }
}

/// Hook event data for before_agent_start
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BeforeAgentStartEvent {
    pub messages: Vec<String>,
    pub system_prompt: String,
}

/// Hook event for before_tool_call
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BeforeToolCallEvent {
    pub tool_name: String,
    pub parameters: Value,
}

/// Hook event for after_tool_call
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AfterToolCallEvent {
    pub tool_name: String,
    pub parameters: Value,
    pub result: ToolResult,
    pub duration_ms: u64,
}

/// Hook event for message_received
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageReceivedEvent {
    pub message: String,
    pub channel: String,
    pub sender: String,
}

/// Hook event for message_sending
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageSendingEvent {
    pub message: String,
    pub channel: String,
}

/// Hook modification response - what hooks can return to modify behavior
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HookModification {
    /// Override system prompt
    pub system_prompt_override: Option<String>,

    /// Prepend context to messages
    pub prepend_context: Option<String>,

    /// Block tool execution
    pub block_tool: Option<bool>,

    /// Reason for blocking
    pub block_reason: Option<String>,

    /// Modify tool parameters
    pub modified_parameters: Option<Value>,
}

/// Plugin hook function
pub type PluginHook = Arc<
    dyn Fn(HookType, ToolContext) -> Pin<Box<dyn Future<Output = Result<Option<HookModification>>> + Send>>
        + Send
        + Sync,
>;

/// Plugin metadata from manifest
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub license: Option<String>,
    pub config_schema: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_type_display() {
        assert_eq!(HookType::BeforeAgentStart.to_string(), "before_agent_start");
        assert_eq!(HookType::BeforeToolCall.to_string(), "before_tool_call");
        assert_eq!(HookType::SessionStart.to_string(), "session_start");
    }

    #[test]
    fn test_tool_context_creation() {
        let ctx = ToolContext {
            session_id: "session-123".to_string(),
            workspace_dir: Some("/workspace".to_string()),
            agent_id: Some("agent-456".to_string()),
            message_channel: Some("telegram".to_string()),
            sandboxed: true,
            metadata: HashMap::new(),
        };

        assert_eq!(ctx.session_id, "session-123");
        assert!(ctx.sandboxed);
    }

    #[test]
    fn test_tool_result_serialization() {
        let result = ToolResult {
            content: "Success".to_string(),
            details: Some(HashMap::from([("key".to_string(), json!("value"))])),
            success: true,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: ToolResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.content, "Success");
        assert!(deserialized.success);
    }
}
