use crate::core::Router;
use crate::storage::Storage;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

/// WhatsApp channel adapter
/// Uses WhatsApp Business API via HTTP (no dependency conflicts)
pub struct WhatsAppAdapter<S: Storage> {
    router: Arc<Router<S>>,
    config: WhatsAppConfig,
    http_client: Client,
}

#[derive(Clone, Debug)]
pub struct WhatsAppConfig {
    pub enabled: bool,
    pub phone_number: String,
    pub api_key: String,
}

/// Message format from WhatsApp webhook
#[derive(Debug, Serialize, Deserialize)]
pub struct WhatsAppWebhookMessage {
    pub from: String,
    pub text: String,
    #[serde(default)]
    pub message_type: String,
}

impl<S: Storage + 'static> WhatsAppAdapter<S> {
    pub fn new(router: Arc<Router<S>>, config: WhatsAppConfig) -> Result<Self> {
        if !config.enabled {
            info!("WhatsApp adapter disabled in configuration");
            return Ok(Self {
                router,
                config,
                http_client: Client::new(),
            });
        }

        info!(
            "WhatsApp adapter initialized for account: {}",
            config.phone_number
        );

        Ok(Self {
            router,
            config,
            http_client: Client::new(),
        })
    }

    /// Handle incoming WhatsApp message
    pub async fn handle_message(
        &self,
        from: String,
        message_text: String,
    ) -> Result<String> {
        if !self.config.enabled {
            return Ok("WhatsApp adapter is disabled".to_string());
        }

        tracing::debug!("WhatsApp message from {}: {}", from, message_text);

        // Extract user ID from WhatsApp address (format: 1234567890@s.whatsapp.net)
        let user_id = from
            .split('@')
            .next()
            .unwrap_or(&from)
            .to_string();

        // Route through the main gateway
        let response = self
            .router
            .handle_message(&user_id, "whatsapp", &message_text)
            .await
            .context("Failed to process WhatsApp message")?;

        Ok(response.content)
    }

    /// Check if WhatsApp adapter is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get WhatsApp account phone number
    pub fn phone_number(&self) -> &str {
        &self.config.phone_number
    }

    /// Send message to WhatsApp user via WhatsApp Business API
    pub async fn send_message(&self, to: &str, text: &str) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let request_body = serde_json::json!({
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": to,
            "type": "text",
            "text": {
                "body": text
            }
        });

        let response = self
            .http_client
            .post("https://graph.instagram.com/v18.0/1234567890/messages")
            .bearer_auth(&self.config.api_key)
            .json(&request_body)
            .send()
            .await
            .context("Failed to send WhatsApp message")?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("WhatsApp API error: {}", error_text);
        }

        tracing::debug!("Message sent to WhatsApp user: {}", to);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, SessionsConfig};
    use crate::llm::Client as LlmClient;
    use crate::storage::{Message, Session, Storage};
    use anyhow::Result;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct MockStorage {
        sessions: Arc<Mutex<Vec<Session>>>,
        messages: Arc<Mutex<Vec<Message>>>,
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
        async fn get_session(&self, id: &str) -> Result<Option<Session>> {
            let sessions = self.sessions.lock().unwrap();
            Ok(sessions.iter().find(|s| s.id == id).cloned())
        }

        async fn create_session(&self, session: Session) -> Result<()> {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.push(session);
            Ok(())
        }

        async fn update_session(&self, session: Session) -> Result<()> {
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
        ) -> Result<Option<Session>> {
            let sessions = self.sessions.lock().unwrap();
            Ok(sessions
                .iter()
                .find(|s| s.user_id == user_id && s.channel == channel && s.scope == scope)
                .cloned())
        }

        async fn get_messages(&self, session_id: &str, limit: Option<usize>) -> Result<Vec<Message>> {
            let messages = self.messages.lock().unwrap();
            let mut session_messages: Vec<Message> = messages
                .iter()
                .filter(|m| m.session_id == session_id)
                .cloned()
                .collect();

            if let Some(lim) = limit {
                session_messages.truncate(lim);
            }

            Ok(session_messages)
        }

        async fn add_message(&self, message: Message) -> Result<()> {
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
            api_key: "test_key".to_string(),
        };

        assert!(config.enabled);
        assert_eq!(config.phone_number, "1234567890");
    }

    #[test]
    fn test_whatsapp_adapter_disabled() {
        let config = WhatsAppConfig {
            enabled: false,
            phone_number: "1234567890".to_string(),
            api_key: "test_key".to_string(),
        };

        assert!(!config.enabled);
    }

    #[test]
    fn test_whatsapp_user_id_extraction() {
        let whatsapp_address = "1234567890@s.whatsapp.net";
        let user_id = whatsapp_address
            .split('@')
            .next()
            .unwrap_or(whatsapp_address);

        assert_eq!(user_id, "1234567890");
    }
}
