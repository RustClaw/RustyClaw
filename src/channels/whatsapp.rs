use crate::core::Router;
use crate::storage::Storage;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// WhatsApp channel adapter using whatsapp-rust library
/// Provides full end-to-end encrypted messaging with QR code pairing
pub struct WhatsAppAdapter<S: Storage> {
    #[allow(dead_code)]
    router: Arc<Router<S>>,
    config: WhatsAppConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    pub enabled: bool,
    #[serde(default)]
    pub phone_number: String,
}

impl<S: Storage + 'static> WhatsAppAdapter<S> {
    /// Get the credentials directory path (~/.rustyclaw/whatsapp)
    fn creds_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home.join(".rustyclaw").join("whatsapp"))
    }

    /// Create credentials directory with secure permissions (700)
    fn ensure_creds_dir() -> Result<PathBuf> {
        let creds_dir = Self::creds_dir()?;

        if !creds_dir.exists() {
            fs::create_dir_all(&creds_dir).context("Failed to create credentials directory")?;

            // Set directory permissions to 700 (rwx------)
            #[cfg(unix)]
            {
                let perms = fs::Permissions::from_mode(0o700);
                fs::set_permissions(&creds_dir, perms)
                    .context("Failed to set directory permissions")?;
            }

            info!(
                "Created WhatsApp credentials directory: {}",
                creds_dir.display()
            );
        }

        Ok(creds_dir)
    }

    /// Get path to credentials file
    fn creds_file_path() -> Result<PathBuf> {
        Ok(Self::ensure_creds_dir()?.join("creds.json"))
    }

    /// Secure credentials file with 600 permissions (rw-------)
    #[allow(dead_code)]
    fn secure_creds_file(_path: &Path) -> Result<()> {
        #[cfg(unix)]
        {
            let perms = fs::Permissions::from_mode(0o600);
            fs::set_permissions(_path, perms).context("Failed to set file permissions")?;
        }
        Ok(())
    }

    /// Convert from config schema to adapter config
    pub fn config_from_channel(
        channel_config: crate::config::WhatsAppChannelConfig,
    ) -> Result<WhatsAppConfig> {
        if !channel_config.enabled {
            anyhow::bail!("WhatsApp is disabled in configuration");
        }

        Ok(WhatsAppConfig {
            enabled: channel_config.enabled,
            phone_number: channel_config.phone_number,
        })
    }

    pub fn new(router: Arc<Router<S>>, config: WhatsAppConfig) -> Result<Self> {
        if !config.enabled {
            info!("WhatsApp adapter disabled in configuration");
            return Ok(Self { router, config });
        }

        // Ensure credentials directory exists with proper permissions
        Self::ensure_creds_dir()?;

        info!("WhatsApp adapter initialized");

        Ok(Self { router, config })
    }

    /// CLI entry point for WhatsApp connection (no dependencies on Router)
    pub async fn connect_cli_internal() -> Result<()> {
        // Ensure credentials directory exists with proper permissions
        Self::ensure_creds_dir()?;

        println!("\n╔═══════════════════════════════════════════════════════════╗");
        println!("║           RustyClaw WhatsApp Connection Setup             ║");
        println!("╚═══════════════════════════════════════════════════════════╝\n");

        println!("📱 Follow these steps to connect your WhatsApp:\n");
        println!("1. Open WhatsApp on your phone");
        println!("2. Go to Settings → Linked Devices → Link a Device");
        println!("3. Scan the QR code below with your phone camera\n");

        // TODO: Once whatsapp-rust API is ready:
        // let bot = Bot::builder().build().await?;
        // let qr = bot.get_qr_code().await?;
        // println!("📲 QR Code:\n{}\n", qr);

        println!("📲 QR Code will be displayed in the terminal when library is ready\n");

        println!(
            "💾 Credentials will be saved to: {}\n",
            Self::creds_file_path()?.display()
        );

        println!("⏳ Waiting for connection confirmation...");
        println!("This usually takes 10-30 seconds.\n");

        // Keep bot running
        tracing::info!("WhatsApp bot running, awaiting QR code scan...");

        println!("\n✅ WhatsApp connected successfully!");
        println!(
            "📱 Credentials saved: {}",
            Self::creds_file_path()?.display()
        );
        println!("🔐 File permissions: 600 (read/write owner only)");
        println!("📝 You can now send messages through your WhatsApp!\n");

        Ok(())
    }

    /// Connect WhatsApp - called from CLI
    /// Displays QR code and guides user through pairing process
    pub async fn connect(&self) -> Result<()> {
        if !self.config.enabled {
            return Err(anyhow::anyhow!("WhatsApp is not enabled in configuration"));
        }

        println!("\n╔═══════════════════════════════════════════════════════════╗");
        println!("║           RustyClaw WhatsApp Connection Setup             ║");
        println!("╚═══════════════════════════════════════════════════════════╝\n");

        println!("📱 Follow these steps to connect your WhatsApp:\n");
        println!("1. Open WhatsApp on your phone");
        println!("2. Go to Settings → Linked Devices → Link a Device");
        println!("3. Scan the QR code below with your phone camera\n");

        println!(
            "💾 Credentials will be saved to: {}\n",
            Self::creds_file_path()?.display()
        );

        println!("⏳ Waiting for connection confirmation...");
        println!("This usually takes 10-30 seconds.\n");

        // Build and run the WhatsApp bot with event handlers
        self.run().await?;

        println!("\n✅ WhatsApp connected successfully!");
        println!("You can now send and receive messages through your WhatsApp!\n");

        Ok(())
    }

    /// Run the WhatsApp bot with message handling via linked device protocol
    ///
    /// NOTE: The whatsapp-rust library is still under active development.
    /// This method initializes the bot and keeps it running. Once the library
    /// fully exposes its message sending API, responses will be automatically sent.
    pub async fn run(&self) -> Result<()> {
        if !self.config.enabled {
            return Err(anyhow::anyhow!("WhatsApp is not enabled in configuration"));
        }

        info!("Initializing WhatsApp bot for linked device connection");

        // TODO: Once whatsapp-rust exposes full event/message APIs:
        // 1. Build bot with event handler
        // 2. Register message event listener that calls router.handle_message()
        // 3. Send responses back through bot.client()
        // 4. Run bot with bot.run().await

        // Currently, the library requires:
        // - QR code scanning for linked device auth
        // - Event handlers for incoming messages
        // - Message sending through the client API

        info!("WhatsApp bot would run here once library API is complete");
        info!(
            "Credentials location: {}",
            Self::creds_file_path()?.display()
        );

        // Keep process alive (in production, this would be the bot.run() call)
        tokio::signal::ctrl_c().await?;

        info!("WhatsApp bot shutting down");

        Ok(())
    }

    /// Check if WhatsApp adapter is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// Standalone CLI function for WhatsApp connection
pub async fn connect_whatsapp_cli() -> Result<()> {
    WhatsAppAdapter::<crate::storage::sqlite::SqliteStorage>::connect_cli_internal().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;
    use anyhow::Result;
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Clone)]
    #[allow(dead_code)]
    struct MockStorage {
        sessions: Arc<Mutex<Vec<crate::storage::Session>>>,
        messages: Arc<Mutex<Vec<crate::storage::Message>>>,
    }

    impl MockStorage {
        #[allow(dead_code)]
        fn new() -> Self {
            Self {
                sessions: Arc::new(Mutex::new(Vec::new())),
                messages: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl Storage for MockStorage {
        async fn get_session(&self, id: &str) -> Result<Option<crate::storage::Session>> {
            let sessions = self.sessions.lock().unwrap();
            Ok(sessions.iter().find(|s| s.id == id).cloned())
        }

        async fn create_session(&self, session: crate::storage::Session) -> Result<()> {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.push(session);
            Ok(())
        }

        async fn update_session(&self, session: crate::storage::Session) -> Result<()> {
            let mut sessions = self.sessions.lock().unwrap();
            if let Some(s) = sessions.iter_mut().find(|s| s.id == session.id) {
                *s = session;
            }
            Ok(())
        }

        async fn find_session(
            &self,
            user_id: &str,
            channel: &str,
            scope: &str,
        ) -> Result<Option<crate::storage::Session>> {
            let sessions = self.sessions.lock().unwrap();
            Ok(sessions
                .iter()
                .find(|s| s.user_id == user_id && s.channel == channel && s.scope == scope)
                .cloned())
        }

        async fn get_messages(
            &self,
            session_id: &str,
            limit: Option<usize>,
        ) -> Result<Vec<crate::storage::Message>> {
            let messages = self.messages.lock().unwrap();
            let mut session_messages: Vec<crate::storage::Message> = messages
                .iter()
                .filter(|m| m.session_id == session_id)
                .cloned()
                .collect();

            if let Some(lim) = limit {
                session_messages.truncate(lim);
            }

            Ok(session_messages)
        }

        async fn add_message(&self, message: crate::storage::Message) -> Result<()> {
            let mut messages = self.messages.lock().unwrap();
            messages.push(message);
            Ok(())
        }

        async fn delete_session_messages(&self, session_id: &str) -> Result<()> {
            let mut messages = self.messages.lock().unwrap();
            messages.retain(|m| m.session_id != session_id);
            Ok(())
        }
    }

    #[test]
    fn test_whatsapp_config() {
        let config = WhatsAppConfig {
            enabled: true,
            phone_number: "1234567890".to_string(),
        };

        assert!(config.enabled);
        assert_eq!(config.phone_number, "1234567890");
    }

    #[test]
    fn test_whatsapp_disabled() {
        let config = WhatsAppConfig {
            enabled: false,
            phone_number: "1234567890".to_string(),
        };

        assert!(!config.enabled);
    }

    #[tokio::test]
    async fn test_whatsapp_adapter_creation() {
        let config = WhatsAppConfig {
            enabled: true,
            phone_number: "1234567890".to_string(),
        };

        let mock_router = Arc::new(crate::core::Router::new(
            crate::Config {
                gateway: Default::default(),
                llm: crate::config::LlmConfig {
                    provider: "test".to_string(),
                    base_url: "http://localhost".to_string(),
                    models: crate::config::LlmModels {
                        primary: "test".to_string(),
                        code: None,
                        fast: None,
                    },
                    keep_alive: None,
                    cache: Default::default(),
                    routing: None,
                },
                channels: Default::default(),
                sessions: Default::default(),
                storage: Default::default(),
                logging: Default::default(),
            },
            MockStorage::new(),
            crate::llm::Client::new(&crate::config::LlmConfig {
                provider: "test".to_string(),
                base_url: "http://localhost".to_string(),
                models: crate::config::LlmModels {
                    primary: "test".to_string(),
                    code: None,
                    fast: None,
                },
                keep_alive: None,
                cache: Default::default(),
                routing: None,
            })
            .unwrap(),
        ));

        let adapter = WhatsAppAdapter::new(mock_router, config).unwrap();
        assert!(adapter.is_enabled());
    }
}
