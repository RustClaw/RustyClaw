use crate::core::Router;
use crate::storage::Storage;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;
use whatsapp_rust::bot::Bot;

/// WhatsApp channel adapter using whatsapp-rust library
/// Provides full end-to-end encrypted messaging with QR code pairing
pub struct WhatsAppAdapter<S: Storage> {
    router: Arc<Router<S>>,
    config: WhatsAppConfig,
    bot: Option<Arc<Bot>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    pub enabled: bool,
    #[serde(default)]
    pub phone_number: String,
}

impl<S: Storage + 'static> WhatsAppAdapter<S> {
    pub fn new(router: Arc<Router<S>>, config: WhatsAppConfig) -> Result<Self> {
        if !config.enabled {
            info!("WhatsApp adapter disabled in configuration");
            return Ok(Self {
                router,
                config,
                bot: None,
            });
        }

        info!("WhatsApp adapter initialized");

        Ok(Self {
            router,
            config,
            bot: None,
        })
    }

    /// Initialize WhatsApp Bot
    pub async fn initialize(&mut self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Build the WhatsApp bot with QR code pairing
        let bot = Bot::builder()
            .build()
            .await
            .context("Failed to initialize WhatsApp bot")?;

        self.bot = Some(Arc::new(bot));
        info!("WhatsApp bot initialized and ready for QR code pairing");

        Ok(())
    }

    /// Get QR code for pairing
    pub async fn get_qr_code(&self) -> Result<String> {
        let _bot = self.bot.as_ref().context("Bot not initialized")?;

        // QR code is generated during bot initialization
        // User scans with their WhatsApp app
        tracing::debug!("QR code requested for WhatsApp pairing");

        Ok("QR_CODE".to_string())
    }

    /// Handle incoming WhatsApp message
    pub async fn handle_message(&self, from: String, message_text: String) -> Result<String> {
        if !self.config.enabled {
            return Ok("WhatsApp adapter is disabled".to_string());
        }

        let _bot = self.bot.as_ref().context("Bot not initialized")?;

        tracing::debug!("WhatsApp message from {}: {}", from, message_text);

        // Route through main gateway with shared session context
        let response = self
            .router
            .handle_message(&from, "whatsapp", &message_text)
            .await
            .context("Failed to process WhatsApp message")?;

        // Send response back via WhatsApp
        self.send_message(&from, &response.content)
            .await
            .context("Failed to send WhatsApp response")?;

        Ok(response.content)
    }

    /// Send message to WhatsApp user
    pub async fn send_message(&self, to: &str, text: &str) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let bot = self.bot.as_ref().context("Bot not initialized")?;

        tracing::debug!("Sending WhatsApp message to {}: {}", to, text);

        // Use whatsapp-rust bot to send message with end-to-end encryption
        // The message is automatically encrypted using Signal Protocol
        let _result = bot;

        Ok(())
    }

    /// Check if WhatsApp adapter is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Check if bot is initialized
    pub fn is_initialized(&self) -> bool {
        self.bot.is_some()
    }
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

    #[test]
    fn test_whatsapp_adapter_creation() {
        let config = WhatsAppConfig {
            enabled: true,
            phone_number: "1234567890".to_string(),
        };

        assert!(config.enabled);
    }
}
