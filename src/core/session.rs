use crate::config::SessionsConfig;
use crate::llm::{ChatMessage, ChatRequest, Client as LlmClient, ToolDefinition};
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

    /// Parse channel routing mode from config
    fn get_channel_routing_mode(&self) -> crate::config::ChannelRoutingMode {
        match self.config.channel_routing.as_str() {
            "shared" => crate::config::ChannelRoutingMode::Shared,
            "bridged" => crate::config::ChannelRoutingMode::Bridged,
            _ => crate::config::ChannelRoutingMode::Isolated, // Default to isolated
        }
    }

    /// Determine effective channel based on routing mode
    fn get_effective_channel(&self, channel: &str) -> String {
        match self.get_channel_routing_mode() {
            crate::config::ChannelRoutingMode::Isolated => {
                // Each channel has separate sessions
                channel.to_string()
            }
            crate::config::ChannelRoutingMode::Shared => {
                // All channels share same session
                "global".to_string()
            }
            crate::config::ChannelRoutingMode::Bridged => {
                // For now, use isolated (bridge lookups would go here)
                channel.to_string()
            }
        }
    }

    /// Get or create a session for a user
    pub async fn get_or_create_session(&self, user_id: &str, channel: &str) -> Result<Session> {
        let scope = &self.config.scope;
        let effective_channel = self.get_effective_channel(channel);

        tracing::info!(
            "Session lookup: user={}, channel={}, effective_channel={}, scope={}, routing={}",
            user_id,
            channel,
            effective_channel,
            scope,
            self.config.channel_routing
        );

        // Try to find existing session
        if let Some(session) = self
            .storage
            .find_session(user_id, &effective_channel, scope)
            .await?
        {
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
            channel: effective_channel.clone(),
            scope: scope.clone(),
            created_at: now,
            updated_at: now,
        };

        self.storage.create_session(storage_session).await?;

        Ok(Session {
            id: session_id,
            user_id: user_id.to_string(),
            channel: effective_channel,
        })
    }

    /// Process a user message and return the assistant's response
    /// This is the main method for handling conversations
    /// Handles tool calling with automatic feedback loops
    pub async fn process_message(
        &self,
        session_id: &str,
        user_message: &str,
    ) -> Result<MessageResponse> {
        // Add user message to storage
        self.add_message(session_id, "user", user_message, None, None)
            .await?;

        // Get tools available
        let tools = self.get_available_tools();

        // Process message through LLM with tool calling
        self.process_with_tools(session_id, tools).await
    }

    /// Process message with tool support
    /// Handles tool calling loops until the model generates final response
    async fn process_with_tools(
        &self,
        session_id: &str,
        tools: Vec<ToolDefinition>,
    ) -> Result<MessageResponse> {
        // Get conversation history
        let history = self
            .storage
            .get_messages(session_id, Some(50))
            .await
            .context("Failed to get message history")?;

        // Convert storage messages to LLM messages
        let mut llm_messages: Vec<ChatMessage> = history
            .iter()
            .map(|msg| ChatMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
            })
            .collect();

        tracing::info!(
            "Processing message for session {}: {} messages in context, {} tools available",
            session_id,
            llm_messages.len(),
            tools.len()
        );

        // Determine model to use (auto-route based on last user message)
        let model = if let Some(last_user_msg) = llm_messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_str())
        {
            self.llm_client.route_model(last_user_msg).to_string()
        } else {
            self.llm_client.primary_model().to_string()
        };

        // Tool calling loop - continue until no more tool calls
        loop {
            // Send request to LLM
            let request = ChatRequest {
                model: model.clone(),
                messages: llm_messages.clone(),
                max_tokens: None,
                temperature: None,
                tools: if tools.is_empty() {
                    None
                } else {
                    Some(tools.clone())
                },
            };

            let response = self
                .llm_client
                .chat(request)
                .await
                .context("Failed to get LLM response")?;

            // Check if we have tool calls to process
            if let Some(tool_calls) = response.tool_calls {
                tracing::info!("LLM generated {} tool calls", tool_calls.len());

                // Add assistant response to message history (contains tool_use)
                llm_messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: response.content.clone(),
                });

                // Execute each tool and collect results
                for tool_call in tool_calls {
                    tracing::info!("Executing tool: {}", tool_call.name);

                    let result =
                        match crate::tools::execute_tool(&tool_call.name, &tool_call.arguments)
                            .await
                        {
                            Ok(result) => {
                                tracing::info!("Tool {} succeeded", tool_call.name);
                                result
                            }
                            Err(err) => {
                                tracing::error!("Tool {} failed: {}", tool_call.name, err);
                                format!("Error: {}", err)
                            }
                        };

                    // Add tool result to message history
                    llm_messages.push(ChatMessage {
                        role: "user".to_string(),
                        content: format!("Tool {} result: {}", tool_call.name, result),
                    });
                }
            } else {
                // No tool calls - this is the final response
                tracing::info!(
                    "Final response generated: model={}, tokens={}",
                    response.model,
                    response.usage.as_ref().map(|u| u.total_tokens).unwrap_or(0)
                );

                // Add final assistant response to storage
                self.add_message(
                    session_id,
                    "assistant",
                    &response.content,
                    Some(&response.model),
                    response.usage.as_ref().map(|u| u.total_tokens),
                )
                .await?;

                return Ok(MessageResponse {
                    content: response.content,
                    model: response.model,
                    tokens: response.usage.map(|u| u.total_tokens),
                });
            }
        }
    }

    /// Get available tools for this session
    fn get_available_tools(&self) -> Vec<ToolDefinition> {
        // For now, always enable WhatsApp tools if service is available
        if crate::get_whatsapp_service().is_some() {
            crate::tools::whatsapp::get_whatsapp_tool_definitions()
        } else {
            Vec::new()
        }
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
