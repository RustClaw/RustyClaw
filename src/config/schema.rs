use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub gateway: GatewayConfig,
    pub llm: LlmConfig,
    #[serde(default)]
    pub channels: ChannelsConfig,
    #[serde(default)]
    pub sessions: SessionsConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub sandbox: SandboxConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            log_level: default_log_level(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    pub models: LlmModels,
    #[serde(default)]
    pub keep_alive: Option<String>,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub routing: Option<RoutingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModels {
    pub primary: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub fast: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_cache_type", rename = "type")]
    pub cache_type: String,
    #[serde(default = "default_max_models")]
    pub max_models: usize,
    #[serde(default = "default_eviction")]
    pub eviction: String,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            cache_type: default_cache_type(),
            max_models: default_max_models(),
            eviction: default_eviction(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub rules: Vec<RoutingRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub pattern: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelsConfig {
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub discord: DiscordConfig,
    #[serde(default)]
    pub whatsapp: WhatsAppChannelConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    pub token: Option<String>,
    #[serde(default)]
    pub allowed_users: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordConfig {
    #[serde(default)]
    pub enabled: bool,
    pub token: Option<String>,
    #[serde(default)]
    pub allowed_users: Vec<u64>,
    #[serde(default)]
    pub allowed_guilds: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WhatsAppChannelConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub phone_number: String,
    #[serde(default)]
    pub local_gateway_url: Option<String>,
    /// Enable self-chat mode (only respond to messages from yourself)
    #[serde(default = "default_self_chat_mode")]
    pub self_chat_mode: bool,
    /// Account ID for multi-account support (defaults to phone number)
    #[serde(default)]
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionsConfig {
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Channel routing mode: isolated, shared, or bridged
    #[serde(default = "default_channel_routing")]
    pub channel_routing: String,
}

/// Channel routing modes for cross-channel context sharing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelRoutingMode {
    /// Each channel has isolated sessions (default)
    Isolated,
    /// All channels share the same session for a user
    Shared,
    /// Channels can be bridged via explicit commands
    Bridged,
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self {
            scope: default_scope(),
            max_tokens: default_max_tokens(),
            channel_routing: default_channel_routing(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_storage_type")]
    pub storage_type: String,
    #[serde(default = "default_storage_path")]
    pub path: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            storage_type: default_storage_type(),
            path: default_storage_path(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
        }
    }
}

// Default functions
fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    18789
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_provider() -> String {
    "ollama".to_string()
}

fn default_base_url() -> String {
    "http://localhost:11434/v1".to_string()
}

fn default_scope() -> String {
    "per-sender".to_string()
}

fn default_max_tokens() -> usize {
    128000
}

fn default_storage_type() -> String {
    "sqlite".to_string()
}

fn default_storage_path() -> String {
    dirs::home_dir()
        .map(|h: std::path::PathBuf| {
            h.join(".rustyclaw")
                .join("data.db")
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_else(|| "./data.db".to_string())
}

fn default_log_format() -> String {
    "pretty".to_string()
}

fn default_cache_type() -> String {
    "ram".to_string()
}

fn default_max_models() -> usize {
    3
}

fn default_eviction() -> String {
    "lru".to_string()
}

fn default_self_chat_mode() -> bool {
    true
}

fn default_channel_routing() -> String {
    "isolated".to_string()
}

// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Sandbox mode: off, non-main, or all
    #[serde(default = "default_sandbox_mode")]
    pub mode: crate::sandbox::SandboxMode,

    /// Container scope: session, agent, or shared
    #[serde(default = "default_sandbox_scope")]
    pub scope: crate::sandbox::ContainerScope,

    /// Docker image to use
    #[serde(default = "default_sandbox_image")]
    pub image: String,

    /// Workspace mode: none, ro (read-only), rw (read-write)
    #[serde(default = "default_workspace_mode")]
    pub workspace: crate::sandbox::WorkspaceMode,

    /// Enable network access for containers
    #[serde(default)]
    pub network: bool,

    /// Setup command to run when container starts
    #[serde(default)]
    pub setup_command: Option<String>,

    /// Custom bind mounts
    #[serde(default)]
    pub mounts: Vec<String>,

    /// Automatic pruning configuration
    #[serde(default)]
    pub pruning: crate::sandbox::PruningConfig,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            mode: default_sandbox_mode(),
            scope: default_sandbox_scope(),
            image: default_sandbox_image(),
            workspace: default_workspace_mode(),
            network: false,
            setup_command: None,
            mounts: vec![],
            pruning: Default::default(),
        }
    }
}

fn default_sandbox_mode() -> crate::sandbox::SandboxMode {
    crate::sandbox::SandboxMode::NonMain
}

fn default_sandbox_scope() -> crate::sandbox::ContainerScope {
    crate::sandbox::ContainerScope::Session
}

fn default_sandbox_image() -> String {
    "ubuntu:22.04".to_string()
}

fn default_workspace_mode() -> crate::sandbox::WorkspaceMode {
    crate::sandbox::WorkspaceMode::None
}

// Tools configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    /// Tool access policies: tool_name -> access_level (allow, deny, elevated)
    #[serde(default)]
    pub policies: HashMap<String, String>,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            policies: HashMap::from([
                ("exec".to_string(), "elevated".to_string()),
                ("bash".to_string(), "elevated".to_string()),
                ("python".to_string(), "elevated".to_string()),
                ("send_whatsapp".to_string(), "allow".to_string()),
                ("list_whatsapp_groups".to_string(), "allow".to_string()),
                ("list_whatsapp_accounts".to_string(), "allow".to_string()),
                ("web_fetch".to_string(), "elevated".to_string()),
                ("web_search".to_string(), "elevated".to_string()),
                ("read_file".to_string(), "elevated".to_string()),
                ("write_file".to_string(), "elevated".to_string()),
                ("list_files".to_string(), "elevated".to_string()),
            ]),
        }
    }
}
