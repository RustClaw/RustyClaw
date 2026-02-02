use crate::config::SessionsConfig;
use crate::llm::{ChatMessage, ChatRequest, Client as LlmClient};
use crate::storage::{Message as StorageMessage, Session as StorageSession, Storage};
use anyhow::{Context, Result};
use chrono::Utc;
use uuid::Uuid;

/// Session manager with LLM integration
#[derive(Clone)]
pub struct SessionManager<S: Storage> {
    storage: S,
    config: SessionsConfig,
    llm_client: LlmClient,
}

#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub channel: String,
}

/// Response from processing a message
#[derive(Debug, Clone)]
pub struct MessageResponse {
    pub content: String,
    pub model: String,
    pub tokens: Option<usize>,
}

impl<S: Storage> SessionManager<S> {
    pub fn new(storage: S, config: SessionsConfig, llm_client: LlmClient) -> Self {
        Self {
            storage,
            config,
            llm_client,
        }
    }

    /// Get or create a session for a user
    pub async fn get_or_create_session(&self, user_id: &str, channel: &str) -> Result<Session> {
        let scope = &self.config.scope;

        // Try to find existing session
        if let Some(session) = self.storage.find_session(user_id, channel, scope).await? {
            // Update last accessed time
            let mut updated = session.clone();
            updated.updated_at = Utc::now();
            self.storage.update_session(updated).await?;

            return Ok(Session {
                id: session.id,
                user_id: session.user_id,
                channel: session.channel,
            });
        }

        // Create new session
        let session_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let storage_session = StorageSession {
            id: session_id.clone(),
            user_id: user_id.to_string(),
            channel: channel.to_string(),
            scope: scope.clone(),
            created_at: now,
            updated_at: now,
        };

        self.storage.create_session(storage_session).await?;

        Ok(Session {
            id: session_id,
            user_id: user_id.to_string(),
            channel: channel.to_string(),
        })
    }

    /// Process a user message and return the assistant's response
    /// This is the main method for handling conversations
    pub async fn process_message(
        &self,
        session_id: &str,
        user_message: &str,
    ) -> Result<MessageResponse> {
        // Add user message to storage
        self.add_message(session_id, "user", user_message, None, None)
            .await?;

        // Get conversation history
        let history = self
            .storage
            .get_messages(session_id, Some(50))
            .await
            .context("Failed to get message history")?;

        // Convert storage messages to LLM messages
        let llm_messages: Vec<ChatMessage> = history
            .iter()
            .map(|msg| ChatMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
            })
            .collect();

        tracing::info!(
            "Processing message for session {}: {} messages in context",
            session_id,
            llm_messages.len()
        );

        // Send to LLM (model will be auto-routed based on content)
        let request = ChatRequest {
            model: String::new(), // Empty = auto-route
            messages: llm_messages,
            max_tokens: None,
            temperature: None,
        };

        let response = self
            .llm_client
            .chat(request)
            .await
            .context("Failed to get LLM response")?;

        let tokens = response.usage.as_ref().map(|u| u.total_tokens);

        // Add assistant response to storage
        self.add_message(
            session_id,
            "assistant",
            &response.content,
            Some(&response.model),
            tokens,
        )
        .await?;

        tracing::info!(
            "Response generated: model={}, tokens={:?}",
            response.model,
            tokens
        );

        Ok(MessageResponse {
            content: response.content,
            model: response.model,
            tokens,
        })
    }

    /// Add a message to a session
    pub async fn add_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        model_used: Option<&str>,
        tokens: Option<usize>,
    ) -> Result<()> {
        let message = StorageMessage {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            created_at: Utc::now(),
            model_used: model_used.map(|s| s.to_string()),
            tokens,
        };

        self.storage.add_message(message).await
    }

    /// Get recent messages for a session
    pub async fn get_messages(&self, session_id: &str) -> Result<Vec<StorageMessage>> {
        self.storage.get_messages(session_id, Some(50)).await
    }

    /// Clear all messages in a session (reset conversation)
    pub async fn clear_session(&self, session_id: &str) -> Result<()> {
        self.storage.delete_session_messages(session_id).await
    }

    /// Get session statistics
    pub async fn get_session_stats(&self, session_id: &str) -> Result<SessionStats> {
        let messages = self.storage.get_messages(session_id, None).await?;

        let total_messages = messages.len();
        let user_messages = messages.iter().filter(|m| m.role == "user").count();
        let assistant_messages = messages.iter().filter(|m| m.role == "assistant").count();
        let total_tokens: usize = messages.iter().filter_map(|m| m.tokens).sum();

        // Count models used
        let mut models_used = std::collections::HashMap::new();
        for msg in messages.iter() {
            if let Some(model) = &msg.model_used {
                *models_used.entry(model.clone()).or_insert(0) += 1;
            }
        }

        Ok(SessionStats {
            total_messages,
            user_messages,
            assistant_messages,
            total_tokens,
            models_used,
        })
    }
}

/// Session statistics
#[derive(Debug, Clone)]
pub struct SessionStats {
    pub total_messages: usize,
    pub user_messages: usize,
    pub assistant_messages: usize,
    pub total_tokens: usize,
    pub models_used: std::collections::HashMap<String, usize>,
}
