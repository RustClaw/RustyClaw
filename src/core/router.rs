use crate::config::Config;
use crate::config::workspace::Workspace;
use crate::core::{MessageResponse, PairingManager, SessionManager};
use crate::llm::Client as LlmClient;
use crate::storage::Storage;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Main router for handling incoming messages from all channels
#[derive(Clone)]
pub struct Router<S: Storage> {
    #[allow(dead_code)]
    config: Arc<RwLock<Config>>,
    session_manager: SessionManager<S>,
    pub pairing_manager: PairingManager<S>,
}

impl<S: Storage + 'static> Router<S> {
    pub async fn new(config: Arc<RwLock<Config>>, storage: S, llm_client: LlmClient) -> Self {
        // Read initial config for workspace setup
        let (workspace_path, _sessions_config, _agents_config) = {
            let cfg = config.read().await; // Use async read
            (cfg.workspace.path.clone(), cfg.sessions.clone(), cfg.agents.clone())
        };

        let workspace = Workspace::new(workspace_path);
        
        // Initialize workspace with default files if needed
        if let Err(e) = workspace.init_default() {
            tracing::warn!("Failed to initialize workspace: {}", e);
        }

        let session_manager =
            SessionManager::new(storage.clone(), config.clone(), llm_client, workspace);
        let pairing_manager = PairingManager::new(storage);

        Self {
            config,
            session_manager,
            pairing_manager,
        }
    }

    pub fn config(&self) -> Arc<RwLock<Config>> {
        self.config.clone()
    }

    pub fn workspace(&self) -> &crate::config::workspace::Workspace {
        self.session_manager.workspace()
    }

    /// Resolve agent ID based on user and channel
    async fn resolve_agent(&self, user_id: &str, channel: &str) -> Option<String> {
        let config = self.config.read().await;
        for (agent_id, agent_config) in &config.agents {
            // Check if this agent claims this channel or user
            if agent_config.channels.iter().any(|c| c == channel || c == user_id) {
                return Some(agent_id.clone());
            }
        }
        None
    }

    /// Handle an incoming message from a user
    pub async fn handle_message(
        &self,
        user_id: &str,
        channel: &str,
        content: &str,
    ) -> Result<MessageResponse> {
        tracing::debug!("Handling message from user {} on {}", user_id, channel);

        // Resolve agent
        let agent_id = self.resolve_agent(user_id, channel).await;
        let agent_id_ref = agent_id.as_deref();

        // Get or create session
        let session = self
            .session_manager
            .get_or_create_session(user_id, channel, agent_id_ref)
            .await?;

        // Process message (SessionManager handles LLM interaction)
        let response = self
            .session_manager
            .process_message(&session.id, content, agent_id_ref)
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
        let agent_id = self.resolve_agent(user_id, channel).await;
        let agent_id_ref = agent_id.as_deref();

        let session = self
            .session_manager
            .get_or_create_session(user_id, channel, agent_id_ref)
            .await?;

        self.session_manager.clear_session(&session.id).await
    }

    /// Get session statistics
    pub async fn get_session_stats(
        &self,
        user_id: &str,
        channel: &str,
    ) -> Result<crate::core::SessionStats> {
        let agent_id = self.resolve_agent(user_id, channel).await;
        let agent_id_ref = agent_id.as_deref();

        let session = self
            .session_manager
            .get_or_create_session(user_id, channel, agent_id_ref)
            .await?;

        self.session_manager.get_session_stats(&session.id).await
    }

    /// Get or create session (exposed for web API)
    pub async fn get_or_create_session_api(
        &self,
        user_id: &str,
        channel: &str,
    ) -> Result<crate::core::Session> {
        let agent_id = self.resolve_agent(user_id, channel).await;
        let agent_id_ref = agent_id.as_deref();

        self.session_manager
            .get_or_create_session(user_id, channel, agent_id_ref)
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
        let agent_id = self.resolve_agent(user_id, channel).await;
        let agent_id_ref = agent_id.as_deref();

        let session = self
            .session_manager
            .get_or_create_session(user_id, channel, agent_id_ref)
            .await?;

        self.session_manager
            .process_message_stream(&session.id, content, agent_id_ref)
            .await
    }
}
