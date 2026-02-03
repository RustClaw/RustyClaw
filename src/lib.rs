pub mod api;
pub mod channels;
pub mod config;
pub mod core;
pub mod llm;
pub mod storage;
pub mod tools;

pub use config::Config;
pub use core::{Router, Session};
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

pub async fn run(config: Config) -> Result<()> {
    tracing::info!("Starting RustyClaw gateway...");

    // Initialize WhatsApp services registry
    init_whatsapp_services();

    // Initialize storage
    let storage = storage::sqlite::SqliteStorage::new(&config.storage.path).await?;
    tracing::info!("Storage initialized");

    // Initialize LLM client
    let llm_client = llm::Client::new(&config.llm)?;
    tracing::info!("LLM client initialized: {}", config.llm.base_url);

    // Initialize router
    let router = Router::new(config.clone(), storage.clone(), llm_client);
    tracing::info!("Router initialized");

    // Start channel adapters
    let mut handles = vec![];

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
