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
use std::sync::Arc;

// Global WhatsApp service registry
static WHATSAPP_SERVICE: OnceCell<Arc<channels::whatsapp::WhatsAppService>> = OnceCell::new();

pub fn set_whatsapp_service(service: Arc<channels::whatsapp::WhatsAppService>) {
    WHATSAPP_SERVICE.set(service).ok();
}

pub fn get_whatsapp_service() -> Option<Arc<channels::whatsapp::WhatsAppService>> {
    WHATSAPP_SERVICE.get().cloned()
}

pub async fn run(config: Config) -> Result<()> {
    tracing::info!("Starting RustyClaw gateway...");

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

    // Wait for all adapters
    tracing::info!("RustyClaw gateway running");
    for handle in handles {
        handle.await??;
    }

    Ok(())
}
