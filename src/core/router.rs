use crate::config::Config;
use crate::core::{MessageResponse, PairingManager, SessionManager};
use crate::llm::Client as LlmClient;
use crate::storage::Storage;
use anyhow::Result;

/// Main router for handling incoming messages from all channels
#[derive(Clone)]
pub struct Router<S: Storage> {
    #[allow(dead_code)]
    config: Config,
    session_manager: SessionManager<S>,
    pub pairing_manager: PairingManager<S>,
}

impl<S: Storage + 'static> Router<S> {
    pub fn new(config: Config, storage: S, llm_client: LlmClient) -> Self {
        let session_manager =
            SessionManager::new(storage.clone(), config.sessions.clone(), llm_client);
        let pairing_manager = PairingManager::new(storage);

        Self {
            config,
            session_manager,
            pairing_manager,
        }
    }

    /// Handle an incoming message from a user
    pub async fn handle_message(
        &self,
        user_id: &str,
        channel: &str,
        content: &str,
    ) -> Result<MessageResponse> {
        tracing::debug!("Handling message from user {} on {}", user_id, channel);

        // Get or create session
        let session = self
            .session_manager
            .get_or_create_session(user_id, channel)
            .await?;

        // Process message (SessionManager handles LLM interaction)
        let response = self
            .session_manager
            .process_message(&session.id, content)
            .await?;

        tracing::info!(
            "Message processed: session={}, model={}, tokens={:?}",
            session.id,
            response.model,
            response.tokens
        );

        Ok(response)
    }

    /// Clear a user's session (reset conversation)
    pub async fn clear_session(&self, user_id: &str, channel: &str) -> Result<()> {
        let session = self
            .session_manager
            .get_or_create_session(user_id, channel)
            .await?;

        self.session_manager.clear_session(&session.id).await
    }

    /// Get session statistics
    pub async fn get_session_stats(
        &self,
        user_id: &str,
        channel: &str,
    ) -> Result<crate::core::SessionStats> {
        let session = self
            .session_manager
            .get_or_create_session(user_id, channel)
            .await?;

        self.session_manager.get_session_stats(&session.id).await
    }

    /// Get or create session (exposed for web API)
    pub async fn get_or_create_session_api(
        &self,
        user_id: &str,
        channel: &str,
    ) -> Result<crate::core::Session> {
        self.session_manager
            .get_or_create_session(user_id, channel)
            .await
    }

    /// Get session messages (exposed for web API)
    pub async fn get_session_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<crate::storage::Message>> {
        self.session_manager.get_messages(session_id).await
    }

    /// Handle message with streaming (returns receiver for StreamEvent)
    pub async fn handle_message_stream(
        &self,
        user_id: &str,
        channel: &str,
        content: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<crate::core::StreamEvent>> {
        let session = self
            .session_manager
            .get_or_create_session(user_id, channel)
            .await?;

        self.session_manager
            .process_message_stream(&session.id, content)
            .await
    }
}
