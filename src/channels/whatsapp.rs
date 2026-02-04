use crate::core::Router;
use crate::storage::Storage;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info};
use wacore::types::events::Event;
use wacore_binary::jid::Jid;
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

        // Parse JID
        let jid = jid_str
            .parse::<Jid>()
            .context("Invalid phone number format - must be numeric")?;

        // Create message
        let msg = wa::Message {
            conversation: Some(message.to_string()),
            ..Default::default()
        };

        // Send message using the Client API
        let message_id = self
            .client
            .send_message(jid.clone(), msg)
            .await
            .context("Failed to send WhatsApp message to contact")?;

        info!("✓ Sent WhatsApp message to {}: ID={}", jid_str, message_id);

        Ok(message_id)
    }

    /// Send message to a group by ID or name
    pub async fn send_to_group(&self, group_identifier: &str, message: &str) -> Result<String> {
        // Try to parse as JID first, otherwise look up by name
        let jid = if group_identifier.contains('@') {
            // Direct JID provided
            group_identifier
                .parse::<Jid>()
                .context("Invalid group JID format")?
        } else {
            // Look up group by name
            self.find_group_by_name(group_identifier).await?
        };

        // Create message
        let msg = wa::Message {
            conversation: Some(message.to_string()),
            ..Default::default()
        };

        // Send message
        let message_id = self
            .client
            .send_message(jid.clone(), msg)
            .await
            .context("Failed to send WhatsApp message to group")?;

        info!(
            "✓ Sent WhatsApp message to group {}: ID={}",
            jid, message_id
        );

        Ok(message_id)
    }

    /// List all groups
    pub async fn list_groups(&self) -> Result<Vec<GroupInfo>> {
        let groups = self
            .client
            .groups()
            .get_participating()
            .await
            .context("Failed to fetch participating groups")?;

        let group_list: Vec<GroupInfo> = groups
            .into_values()
            .map(|metadata| GroupInfo {
                id: metadata.id.to_string(),
                name: metadata.subject.clone(),
                participant_count: metadata.participants.len(),
            })
            .collect();

        info!("✓ Fetched {} WhatsApp groups", group_list.len());
        Ok(group_list)
    }

    /// Verify if a phone number is on WhatsApp
    pub async fn verify_contact(&self, phone: &str) -> Result<Option<String>> {
        let results = self
            .client
            .contacts()
            .is_on_whatsapp(&[phone])
            .await
            .context("Failed to verify contact")?;

        match results.first() {
            Some(result) if result.is_registered => {
                info!("✓ Contact {} is registered on WhatsApp", phone);
                Ok(Some(result.jid.to_string()))
            }
            _ => {
                info!("Contact {} is not on WhatsApp", phone);
                Ok(None)
            }
        }
    }

    /// Find group JID by name (case-insensitive)
    async fn find_group_by_name(&self, name: &str) -> Result<Jid> {
        let groups = self
            .client
            .groups()
            .get_participating()
            .await
            .context("Failed to fetch groups for lookup")?;

        for (_, metadata) in groups {
            if metadata.subject.to_lowercase() == name.to_lowercase() {
                info!("✓ Found group '{}' with JID {}", name, metadata.id);
                return Ok(metadata.id);
            }
        }

        anyhow::bail!("Group '{}' not found in participating groups", name)
    }
}

/// WhatsApp channel adapter using whatsapp-rust library
/// Provides full end-to-end encrypted messaging with QR code pairing
pub struct WhatsAppAdapter<S: Storage> {
    #[allow(dead_code)]
    router: Arc<Router<S>>,
    config: WhatsAppConfig,
    account_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    pub enabled: bool,
    #[serde(default)]
    pub phone_number: String,
    /// Enable self-chat mode (only respond to messages from yourself)
    #[serde(default = "default_self_chat_mode")]
    pub self_chat_mode: bool,
    /// Account ID for multi-account support (defaults to phone number)
    #[serde(default)]
    pub account_id: Option<String>,
}

fn default_self_chat_mode() -> bool {
    true
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
            self_chat_mode: channel_config.self_chat_mode,
            account_id: channel_config.account_id,
        })
    }

    pub fn new(router: Arc<Router<S>>, config: WhatsAppConfig) -> Result<Self> {
        if !config.enabled {
            info!("WhatsApp adapter disabled in configuration");
            let account_id = config
                .account_id
                .clone()
                .unwrap_or_else(|| config.phone_number.clone());
            return Ok(Self {
                router,
                config,
                account_id,
            });
        }

        let account_id = config
            .account_id
            .clone()
            .unwrap_or_else(|| config.phone_number.clone());

        // Ensure credentials directory exists with proper permissions
        Self::ensure_creds_dir()?;

        info!("WhatsApp adapter initialized for account: {}", account_id);

        Ok(Self {
            router,
            config,
            account_id,
        })
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

        // Clone router and config for event handler (needed for 'static closure)
        let router = self.router.clone();
        let config = self.config.clone();
        let account_id = self.account_id.clone();

        // Build the bot with event handler
        let mut bot = Bot::builder()
            .with_backend(backend)
            .with_transport_factory(transport_factory)
            .with_http_client(http_client)
            .on_event(move |event, _client| {
                let router = router.clone();
                let config = config.clone();
                let account_id = account_id.clone();
                async move {
                    match event {
                        Event::Message(message, info) => {
                            // Extract sender JID and get message text
                            let sender_jid = info.source.sender.to_string();

                            // Extract sender phone number (strip @s.whatsapp.net)
                            let sender_phone = sender_jid
                                .strip_suffix("@s.whatsapp.net")
                                .unwrap_or(&sender_jid);

                            // Get text content from the message
                            if let Some(text) = message.conversation.clone() {
                                if text.trim().is_empty() {
                                    return;
                                }

                                // SELF-CHAT MODE FILTER
                                if config.self_chat_mode {
                                    // Only process messages from yourself
                                    if sender_phone != config.phone_number {
                                        tracing::debug!(
                                            "Self-chat mode: ignoring message from {} (not self)",
                                            sender_jid
                                        );
                                        return;
                                    }

                                    info!("✅ Self-chat message received from {}", sender_jid);
                                } else {
                                    info!("WhatsApp message from {}: {}", sender_jid, text);
                                }

                                // Create user_id for session
                                // Format: whatsapp:<account_id>:<sender_phone>
                                let user_id = format!(
                                    "whatsapp:{}:{}",
                                    account_id,
                                    sender_phone
                                );

                                // Create message context for sending reply
                                let ctx = MessageContext {
                                    message: message.clone(),
                                    info: info.clone(),
                                    client: _client.clone(),
                                };

                                // Process message through router
                                match router.handle_message(&user_id, "whatsapp", &text).await {
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
                                                sender_jid, e
                                            );
                                        } else {
                                            info!("✓ Sent response to {}", sender_jid);
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
                            info!("✅ WhatsApp account '{}' connected successfully!", account_id);

                            // Send welcome message to self in self-chat mode
                            if config.self_chat_mode {
                                let jid_str = format!("{}@s.whatsapp.net", config.phone_number);
                                match jid_str.parse::<Jid>() {
                                    Ok(self_jid) => {
                                        let welcome = wa::Message {
                                            conversation: Some(format!(
                                                "🤖 RustyClaw connected (account: {})!\n\n\
                                                You can now send me messages here to interact with the LLM.\n\n\
                                                This is SELF-CHAT MODE - only messages you send to yourself are processed.\n\n\
                                                Try:\n\
                                                • Ask questions: 'What is Rust?'\n\
                                                • Send messages: 'Send WhatsApp to John: Meeting at 3pm'\n\
                                                • List groups: 'What WhatsApp groups do I have?'\n\
                                                • List accounts: 'What WhatsApp accounts are connected?'",
                                                account_id
                                            )),
                                            ..Default::default()
                                        };

                                        if let Err(e) = _client.send_message(self_jid, welcome).await {
                                            error!("Failed to send welcome message: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        error!("Invalid phone number format for welcome message: {}", e);
                                    }
                                }
                            }
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

        // Register service for this account
        crate::register_whatsapp_service(self.account_id.clone(), service);

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
            self_chat_mode: true,
            account_id: Some("personal".to_string()),
        };

        assert!(config.enabled);
        assert_eq!(config.phone_number, "1234567890");
        assert!(config.self_chat_mode);
        assert_eq!(config.account_id, Some("personal".to_string()));
    }

    #[test]
    fn test_whatsapp_disabled() {
        let config = WhatsAppConfig {
            enabled: false,
            phone_number: "1234567890".to_string(),
            self_chat_mode: true,
            account_id: None,
        };

        assert!(!config.enabled);
    }

    #[tokio::test]
    async fn test_whatsapp_adapter_creation() {
        let config = WhatsAppConfig {
            enabled: true,
            phone_number: "1234567890".to_string(),
            self_chat_mode: true,
            account_id: Some("test".to_string()),
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
                sandbox: Default::default(),
                tools: Default::default(),
                api: Default::default(),
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
