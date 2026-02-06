pub mod api;
pub mod channels;
pub mod config;
pub mod core;
pub mod llm;
pub mod plugins;
pub mod sandbox;
pub mod storage;
pub mod tools;

pub use config::Config;
pub use core::{Router, Session};
pub use sandbox::SandboxManager;
pub use storage::Storage;

use anyhow::Result;
use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// Type alias for WhatsApp services registry
type WhatsAppServicesRegistry =
    Arc<RwLock<HashMap<String, Arc<channels::whatsapp::WhatsAppService>>>>;

// Global WhatsApp service registry (supports multiple accounts)
static WHATSAPP_SERVICES: OnceCell<WhatsAppServicesRegistry> = OnceCell::new();

// Global sandbox manager
static SANDBOX_MANAGER: OnceCell<Arc<SandboxManager>> = OnceCell::new();

// Global tool policy engine
static TOOL_POLICY_ENGINE: OnceCell<Arc<tools::policy::ToolPolicyEngine>> = OnceCell::new();

/// Initialize the global WhatsApp services registry
pub fn init_whatsapp_services() {
    WHATSAPP_SERVICES.get_or_init(|| Arc::new(RwLock::new(HashMap::new())));
}

/// Register a WhatsApp service for a specific account
pub fn register_whatsapp_service(
    account_id: String,
    service: Arc<channels::whatsapp::WhatsAppService>,
) {
    init_whatsapp_services();

    let services = WHATSAPP_SERVICES
        .get()
        .expect("WhatsApp services not initialized");

    let mut services = services.write().expect("Failed to acquire write lock");
    services.insert(account_id.clone(), service);

    tracing::info!("✅ Registered WhatsApp service for account: {}", account_id);
}

/// Get WhatsApp service for a specific account
pub fn get_whatsapp_service_by_account(
    account_id: &str,
) -> Option<Arc<channels::whatsapp::WhatsAppService>> {
    let services = WHATSAPP_SERVICES.get()?;
    let services = services.read().expect("Failed to acquire read lock");
    services.get(account_id).cloned()
}

/// Get default WhatsApp service (first registered account)
pub fn get_whatsapp_service() -> Option<Arc<channels::whatsapp::WhatsAppService>> {
    let services = WHATSAPP_SERVICES.get()?;
    let services = services.read().expect("Failed to acquire read lock");
    services.values().next().cloned()
}

/// List all registered WhatsApp accounts
pub fn list_whatsapp_accounts() -> Vec<String> {
    match WHATSAPP_SERVICES.get() {
        Some(services) => {
            let services = services.read().expect("Failed to acquire read lock");
            services.keys().cloned().collect()
        }
        None => vec![],
    }
}

// Backward compatibility: set service for single account
pub fn set_whatsapp_service(service: Arc<channels::whatsapp::WhatsAppService>) {
    init_whatsapp_services();

    let services = WHATSAPP_SERVICES
        .get()
        .expect("WhatsApp services not initialized");

    let mut services = services.write().expect("Failed to acquire write lock");

    // Use "default" as account_id for backward compatibility
    services.insert("default".to_string(), service);
}

/// Get the global sandbox manager
pub fn get_sandbox_manager() -> Option<Arc<SandboxManager>> {
    SANDBOX_MANAGER.get().cloned()
}

/// Get the global tool policy engine
pub fn get_tool_policy_engine() -> Option<Arc<tools::policy::ToolPolicyEngine>> {
    TOOL_POLICY_ENGINE.get().cloned()
}

pub async fn run(config: Config) -> Result<()> {
    tracing::info!("Starting RustyClaw gateway...");

    // Initialize WhatsApp services registry
    init_whatsapp_services();

    // Initialize sandbox manager if sandboxing is not disabled
    if config.sandbox.mode != sandbox::SandboxMode::Off {
        tracing::info!(
            "Initializing sandbox manager (mode: {:?})",
            config.sandbox.mode
        );
        let sandbox = SandboxManager::new(config.sandbox.clone()).await?;
        SANDBOX_MANAGER.set(Arc::new(sandbox)).ok();
        tracing::info!("✅ Sandbox manager initialized");
    } else {
        tracing::info!("Sandbox disabled (mode: off)");
    }

    // Initialize tool policy engine
    let mut policies = std::collections::HashMap::new();
    for (tool, level_str) in &config.tools.policies {
        if let Ok(level) = level_str.parse::<tools::policy::ToolAccessLevel>() {
            policies.insert(tool.clone(), level);
        }
    }
    let policy_engine = tools::policy::ToolPolicyEngine::with_policies(policies);
    TOOL_POLICY_ENGINE.set(Arc::new(policy_engine)).ok();
    tracing::info!("✅ Tool policy engine initialized");

    // Initialize plugin registry
    let _plugin_registry = plugins::init_plugin_registry();
    tracing::info!("✅ Plugin registry initialized");

    // Initialize and start skill watcher if enabled
    if config.tools.skills_enabled {
        let skills_dir = config.tools.skills_dir.clone();
        tokio::spawn(async move {
            let watcher = tools::skill_watcher::SkillWatcher::new(&skills_dir);
            if let Err(e) = watcher.run().await {
                tracing::error!("Skill watcher error: {}", e);
            }
        });
        tracing::info!(
            "✅ Skill watcher started (dir: {})",
            config.tools.skills_dir
        );
    }

    // Initialize storage
    let storage = storage::sqlite::SqliteStorage::new(&config.storage.path).await?;
    tracing::info!("Storage initialized");

    // Initialize LLM client
    let llm_client = llm::Client::new(&config.llm)?;
    tracing::info!("LLM client initialized: {}", config.llm.base_url);

    // Initialize router
    let router = Router::new(config.clone(), storage.clone(), llm_client);
    tracing::info!("Router initialized");

    // Check if initial setup is needed
    if let Err(e) = router.pairing_manager.check_and_start_setup().await {
        tracing::error!("Failed to check setup state: {}", e);
    }

    // Start channel adapters
    let mut handles = vec![];

    // Start Web API if enabled
    if config.api.enabled {
        tracing::info!(
            "Starting Web API on {}:{}",
            config.api.host,
            config.api.port
        );
        let api_adapter = api::WebApiAdapter::new(
            Arc::new(router.clone()),
            config.api.host.clone(),
            config.api.port,
            config.api.tokens.clone(),
            storage.clone(),
        );
        let api_handle = tokio::spawn(async move { api_adapter.start().await });
        handles.push(api_handle);
    }

    if config.channels.telegram.enabled {
        tracing::info!("Starting Telegram adapter...");
        let telegram_handle = tokio::spawn(channels::telegram::run(
            config.channels.telegram.clone(),
            router.clone(),
        ));
        handles.push(telegram_handle);
    }

    if config.channels.discord.enabled {
        tracing::info!("Starting Discord adapter...");
        let discord_handle = tokio::spawn(channels::discord::run(
            config.channels.discord.clone(),
            router.clone(),
        ));
        handles.push(discord_handle);
    }

    if config.channels.whatsapp.enabled {
        tracing::info!("Starting WhatsApp adapter...");

        let whatsapp_handle = {
            let router = router.clone();
            let whatsapp_cfg = config.channels.whatsapp.clone();
            tokio::spawn(async move {
                let cfg = channels::whatsapp::WhatsAppAdapter::<storage::sqlite::SqliteStorage>::config_from_channel(whatsapp_cfg)?;
                let adapter = channels::whatsapp::WhatsAppAdapter::new(Arc::new(router), cfg)?;
                adapter.run().await
            })
        };

        handles.push(whatsapp_handle);
    }

    // Wait for all adapters
    tracing::info!("RustyClaw gateway running");
    for handle in handles {
        handle.await??;
    }

    Ok(())
}
pub mod mcp;
