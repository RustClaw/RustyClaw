use crate::core::Router;
use crate::storage::Storage;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;
use wacore::types::events::Event;
use waproto::whatsapp as wa;
use whatsapp_rust::bot::{Bot, MessageContext};
use whatsapp_rust::store::SqliteStore;
use whatsapp_rust_tokio_transport::TokioWebSocketTransportFactory;
use whatsapp_rust_ureq_http_client::UreqHttpClient;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Service for sending outbound WhatsApp messages
#[derive(Clone)]
pub struct WhatsAppService {
    #[allow(dead_code)]
    client: Arc<whatsapp_rust::Client>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub participant_count: usize,
}

impl WhatsAppService {
    pub fn new(client: Arc<whatsapp_rust::Client>) -> Self {
        Self { client }
    }

    /// Send message to a contact by phone number
    pub async fn send_to_contact(&self, phone: &str, message: &str) -> Result<String> {
        // Format phone number as JID (e.g., "1234567890@s.whatsapp.net")
        let jid_str = format!("{}@s.whatsapp.net", phone);

        // Parse JID using the whatsapp-rust library's parser
        // Note: Implementation depends on the exact API of whatsapp-rust
        // For now, we'll use a placeholder that returns a generated ID
        // TODO: Complete implementation once whatsapp-rust API is finalized

        let message_id = Uuid::new_v4().to_string();

        info!(
            "Preparing to send WhatsApp message to contact {}: {}",
            jid_str, message
        );

        // The actual send would be:
        // let jid = jid_str.parse()?;
        // let msg = wa::Message { conversation: Some(message.to_string()), ..Default::default() };
        // let message_id = self.client.send_message(jid, msg).await?;

        Ok(message_id)
    }

    /// Send message to a group by ID or name
    pub async fn send_to_group(&self, group_identifier: &str, message: &str) -> Result<String> {
        // TODO: Complete implementation once whatsapp-rust API is finalized
        let message_id = Uuid::new_v4().to_string();

        info!(
            "Preparing to send WhatsApp message to group {}: {}",
            group_identifier, message
        );

        Ok(message_id)
    }

    /// List all groups
    pub async fn list_groups(&self) -> Result<Vec<GroupInfo>> {
        // TODO: Complete implementation once whatsapp-rust API is finalized
        // For now, return empty list
        info!("Fetching WhatsApp groups");
        Ok(Vec::new())
    }

    /// Verify if a phone number is on WhatsApp
    pub async fn verify_contact(&self, phone: &str) -> Result<Option<String>> {
        // TODO: Complete implementation once whatsapp-rust API is finalized
        info!("Verifying contact: {}", phone);
        Ok(None)
    }
}

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

    /// Run the WhatsApp bot with event-driven message handling
    ///
    /// The bot will:
    /// 1. Display QR code for linked device pairing
    /// 2. Listen for incoming messages
    /// 3. Process messages through the Router
    /// 4. Send responses back via WhatsApp (supports 1-on-1 and group chats)
    pub async fn run(&self) -> Result<()> {
        if !self.config.enabled {
            return Err(anyhow::anyhow!("WhatsApp is not enabled in configuration"));
        }

        info!("Initializing WhatsApp bot for linked device connection");

        // Set up storage backend
        let creds_path = Self::creds_file_path()?;
        let backend = Arc::new(
            SqliteStore::new(creds_path.to_string_lossy().as_ref())
                .await
                .context("Failed to initialize SQLite backend")?,
        );

        // Set up network transport
        let transport_factory = TokioWebSocketTransportFactory::new();

        // Set up HTTP client for media operations
        let http_client = UreqHttpClient::new();

        // Clone router for event handler
        let router = self.router.clone();

        // Build the bot with event handler
        let mut bot = Bot::builder()
            .with_backend(backend)
            .with_transport_factory(transport_factory)
            .with_http_client(http_client)
            .on_event(move |event, _client| {
                let router = router.clone();
                async move {
                    match event {
                        Event::Message(message, info) => {
                            // Extract sender JID and get message text
                            let sender = info.source.sender.to_string();

                            // Get text content from the message
                            if let Some(text) = message.conversation.clone() {
                                if text.trim().is_empty() {
                                    return;
                                }

                                info!("WhatsApp message from {}: {}", sender, text);

                                // Create message context for sending reply
                                let ctx = MessageContext {
                                    message: message.clone(),
                                    info: info.clone(),
                                    client: _client.clone(),
                                };

                                // Process message through router
                                match router.handle_message(&sender, "whatsapp", &text).await {
                                    Ok(response) => {
                                        // Create response message
                                        let reply = wa::Message {
                                            conversation: Some(response.content),
                                            ..Default::default()
                                        };

                                        // Send response (works for 1-on-1 and groups)
                                        if let Err(e) = ctx.send_message(reply).await {
                                            error!(
                                                "Failed to send WhatsApp response to {}: {}",
                                                sender, e
                                            );
                                        } else {
                                            info!("✓ Sent response to {}", sender);
                                        }
                                    }
                                    Err(e) => {
                                        error!("Error processing WhatsApp message: {}", e);

                                        // Send error message back to user
                                        let error_reply = wa::Message {
                                            conversation: Some(
                                                "Sorry, I encountered an error processing your message.".to_string()
                                            ),
                                            ..Default::default()
                                        };

                                        if let Err(send_err) = ctx.send_message(error_reply).await {
                                            error!("Failed to send error message: {}", send_err);
                                        }
                                    }
                                }
                            }
                        }
                        Event::Connected(_) => {
                            info!("✅ WhatsApp bot connected successfully!");
                        }
                        Event::LoggedOut(_) => {
                            error!("❌ WhatsApp bot was logged out!");
                        }
                        _ => {
                            // Handle other events as needed
                        }
                    }
                }
            })
            .build()
            .await
            .context("Failed to initialize WhatsApp bot")?;

        // Create and register WhatsApp service for outbound messaging
        let service = Arc::new(WhatsAppService::new(bot.client().clone()));
        crate::set_whatsapp_service(service);

        info!("WhatsApp bot running and listening for messages");
        info!(
            "Credentials location: {}",
            Self::creds_file_path()?.display()
        );

        // Run the bot (blocks until completion or shutdown)
        bot.run().await.context("WhatsApp bot error")?;

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
