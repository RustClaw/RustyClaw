use crate::config::Config;
use crate::core::SessionManager;
use crate::llm::{ChatMessage, ChatRequest, Client as LlmClient};
use crate::storage::Storage;
use anyhow::Result;

#[derive(Clone)]
pub struct Router<S: Storage> {
    #[allow(dead_code)]
    config: Config,
    session_manager: SessionManager<S>,
    llm_client: LlmClient,
}

impl<S: Storage> Router<S> {
    pub fn new(config: Config, storage: S, llm_client: LlmClient) -> Self {
        let session_manager = SessionManager::new(storage, config.sessions.clone());

        Self {
            config,
            session_manager,
            llm_client,
        }
    }

    pub async fn handle_message(
        &self,
        user_id: &str,
        channel: &str,
        content: &str,
    ) -> Result<String> {
        tracing::debug!("Handling message from user {} on {}", user_id, channel);

        // Get or create session
        let session = self
            .session_manager
            .get_or_create_session(user_id, channel)
            .await?;

        // Add user message to session
        self.session_manager
            .add_message(&session.id, "user", content)
            .await?;

        // Get conversation history
        let messages = self.session_manager.get_messages(&session.id).await?;

        // Convert to chat messages
        let chat_messages: Vec<ChatMessage> = messages
            .iter()
            .map(|m| ChatMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();

        // Call LLM
        let request = ChatRequest {
            model: self.llm_client.primary_model().to_string(),
            messages: chat_messages,
            max_tokens: Some(4096),
            temperature: Some(0.7),
        };

        let response = self.llm_client.chat(request).await?;

        // Save assistant response
        self.session_manager
            .add_message(&session.id, "assistant", &response.content)
            .await?;

        Ok(response.content)
    }

    pub async fn clear_session(&self, user_id: &str, channel: &str) -> Result<()> {
        let session = self
            .session_manager
            .get_or_create_session(user_id, channel)
            .await?;

        self.session_manager.clear_session(&session.id).await
    }
}
