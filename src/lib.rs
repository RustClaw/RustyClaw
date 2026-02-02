pub mod api;
pub mod channels;
pub mod config;
pub mod core;
pub mod llm;
pub mod storage;

pub use config::Config;
pub use core::{Router, Session};
pub use storage::Storage;

use anyhow::Result;

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

    // Wait for all adapters
    tracing::info!("RustyClaw gateway running");
    for handle in handles {
        handle.await??;
    }

    Ok(())
}
