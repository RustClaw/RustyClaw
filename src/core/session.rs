use crate::config::SessionsConfig;
use crate::llm::{ChatMessage, ChatRequest, Client as LlmClient, ToolDefinition};
use crate::storage::{Message as StorageMessage, Session as StorageSession, Storage};
use anyhow::{Context, Result};
use chrono::Utc;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Stream events sent from process_message_stream
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Content token(s) from LLM
    Delta(String),
    /// About to execute a tool
    ToolStart { name: String },
    /// Tool finished executing
    ToolEnd { name: String, result: String },
    /// Streaming finished
    Done {
        model: String,
        usage: Option<crate::llm::TokenUsage>,
    },
    /// Error occurred
    Error(String),
}

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

impl<S: Storage + 'static> SessionManager<S> {
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
        let tools = self.get_available_tools().await;

        // Process message through LLM with tool calling
        self.process_with_tools(session_id, tools).await
    }

    /// Process a user message with streaming (returns receiver for StreamEvent)
    /// Note: This method is only available when S: 'static (defined in separate impl block)
    pub async fn process_message_stream_unimplemented(
        &self,
        _session_id: &str,
        _user_message: &str,
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        Err(anyhow::anyhow!("Not implemented for non-static storage"))
    }
}

impl<S: Storage + 'static> SessionManager<S> {
    /// Process a user message with streaming (returns receiver for StreamEvent)
    pub async fn process_message_stream(
        &self,
        session_id: &str,
        user_message: &str,
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        // Add user message to storage
        self.add_message(session_id, "user", user_message, None, None)
            .await?;

        // Get tools available
        let tools = self.get_available_tools().await;

        // Create channel for streaming events
        let (tx, rx) = mpsc::channel::<StreamEvent>(32);

        // Clone what we need for the spawned task
        let storage = self.storage.clone();
        let llm_client = self.llm_client.clone();
        let session_id = session_id.to_string();

        // Spawn streaming task
        tokio::spawn(async move {
            if let Err(e) =
                process_message_stream_task(storage, llm_client, session_id, tools, tx).await
            {
                tracing::error!("Error in streaming task: {}", e);
            }
        });

        Ok(rx)
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

                    let result = match crate::tools::executor::execute_tool_with_context(
                        &tool_call.name,
                        &tool_call.arguments,
                        Some(session_id),
                        true, // In session manager, this is usually the main session
                    )
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
    pub async fn get_available_tools(&self) -> Vec<ToolDefinition> {
        let mut tools = Vec::new();

        // Helper function to extract tool definition from JSON
        let extract_tool = |def: &serde_json::Value| -> Option<ToolDefinition> {
            let func = def.get("function")?;
            let name = func.get("name")?.as_str()?;
            let description = func.get("description")?.as_str()?;
            let parameters = func.get("parameters")?;
            Some(ToolDefinition {
                name: name.to_string(),
                description: description.to_string(),
                parameters: parameters.clone(),
            })
        };

        // 1. Add exec tools (always available)
        let exec_defs = crate::tools::get_exec_tool_definitions();
        for def in exec_defs {
            if let Some(tool) = extract_tool(&def) {
                tools.push(tool);
            }
        }

        // 1b. Add creator tools (always available)
        let creator_defs = crate::tools::get_creator_tool_definitions();
        for def in creator_defs {
            if let Some(tool) = extract_tool(&def) {
                tools.push(tool);
            }
        }

        // 1c. Add web tools (always available)
        let web_defs = crate::tools::web::get_web_tool_definitions();
        for def in web_defs {
            if let Some(tool) = extract_tool(&def) {
                tools.push(tool);
            }
        }

        // 2. Add WhatsApp tools if service is available
        if crate::get_whatsapp_service().is_some() {
            let whatsapp_defs = crate::tools::whatsapp::get_whatsapp_tool_definitions();
            tools.extend(whatsapp_defs);
        }

        // 3. Add plugin tools from PluginRegistry if available
        if let Some(registry) = crate::plugins::get_plugin_registry() {
            if let Ok(tool_names) = registry.tools.list_tools() {
                for tool_name in tool_names {
                    if let Ok(Some(tool)) = registry.tools.get_tool(&tool_name) {
                        tools.push(ToolDefinition {
                            name: tool.name,
                            description: tool.description,
                            parameters: tool.parameters,
                        });
                    }
                }
            }
        }

        // 4. Add skill tools from SKILL_BODIES if available
        tools.extend(get_skill_tool_definitions().await);

        tools
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

/// Helper function to convert skill entries to tool definitions
async fn get_skill_tool_definitions() -> Vec<ToolDefinition> {
    crate::tools::skills::list_skills()
        .await
        .into_iter()
        .map(|entry| ToolDefinition {
            name: entry.manifest.name,
            description: entry.manifest.description,
            parameters: entry.manifest.parameters,
        })
        .collect()
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

/// Accumulated tool call during streaming
struct AccumulatedToolCall {
    id: String,
    name: String,
    arguments: String,
}

/// Streaming task worker function
async fn process_message_stream_task<S: Storage + 'static>(
    storage: S,
    llm_client: crate::llm::Client,
    session_id: String,
    tools: Vec<ToolDefinition>,
    tx: mpsc::Sender<StreamEvent>,
) -> Result<()> {
    use futures::StreamExt;
    use std::collections::HashMap;

    // Get conversation history
    let history = storage
        .get_messages(&session_id, Some(50))
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
        "Starting streaming for session {}: {} messages in context, {} tools available",
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
        llm_client.route_model(last_user_msg).to_string()
    } else {
        llm_client.primary_model().to_string()
    };

    // Tool calling loop - continue until no more tool calls
    loop {
        // Send request to LLM with streaming
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

        let mut stream = match llm_client.chat_stream(request).await {
            Ok(s) => s,
            Err(e) => {
                let _ = tx
                    .send(StreamEvent::Error(format!("LLM error: {}", e)))
                    .await;
                return Err(e);
            }
        };

        // Accumulate content and tool calls during streaming
        let mut content_buf = String::new();
        let mut tool_calls_map: HashMap<usize, AccumulatedToolCall> = HashMap::new();
        let mut finish_reason_: Option<String> = None;
        let mut final_usage = None;

        // Consume the stream
        while let Some(result) = stream.next().await {
            match result {
                Ok(chunk) => {
                    // Accumulate content
                    if let Some(content) = &chunk.content {
                        if !content.is_empty() {
                            content_buf.push_str(content);
                            // Send delta event (per-token)
                            if tx.send(StreamEvent::Delta(content.clone())).await.is_err() {
                                // Receiver dropped - client disconnected
                                return Ok(());
                            }
                        }
                    }

                    // Accumulate tool calls
                    if let Some(tool_calls) = &chunk.tool_calls {
                        for tc in tool_calls {
                            let entry = tool_calls_map.entry(tc.index).or_insert_with(|| {
                                AccumulatedToolCall {
                                    id: tc.id.clone().unwrap_or_default(),
                                    name: tc.name.clone().unwrap_or_default(),
                                    arguments: String::new(),
                                }
                            });

                            if let Some(id) = &tc.id {
                                entry.id = id.clone();
                            }
                            if let Some(name) = &tc.name {
                                entry.name = name.clone();
                            }
                            if let Some(args) = &tc.arguments {
                                entry.arguments.push_str(args);
                            }
                        }
                    }

                    // Track finish reason
                    if let Some(reason) = &chunk.finish_reason {
                        finish_reason_ = Some(reason.clone());
                    }

                    // Track usage
                    if let Some(usage) = &chunk.usage {
                        final_usage = Some(usage.clone());
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(StreamEvent::Error(format!("Stream error: {}", e)))
                        .await;
                    return Err(e);
                }
            }
        }

        // Check finish reason to determine if we have tool calls
        if finish_reason_.as_deref() == Some("tool_calls") && !tool_calls_map.is_empty() {
            tracing::info!("Streaming generated {} tool calls", tool_calls_map.len());

            // Add assistant response to message history
            llm_messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: content_buf.clone(),
            });

            // Execute tools (sort by index)
            let mut sorted_tools: Vec<_> = tool_calls_map.into_iter().collect();
            sorted_tools.sort_by_key(|a| a.0);

            for (_idx, tool_call) in sorted_tools {
                tracing::info!("Executing tool: {}", tool_call.name);

                // Send tool start event
                if tx
                    .send(StreamEvent::ToolStart {
                        name: tool_call.name.clone(),
                    })
                    .await
                    .is_err()
                {
                    // Receiver dropped
                    return Ok(());
                }

                // Execute tool
                let result = match crate::tools::executor::execute_tool_with_context(
                    &tool_call.name,
                    &tool_call.arguments,
                    Some(&session_id),
                    true,
                )
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

                // Send tool end event
                if tx
                    .send(StreamEvent::ToolEnd {
                        name: tool_call.name.clone(),
                        result: result.clone(),
                    })
                    .await
                    .is_err()
                {
                    // Receiver dropped
                    return Ok(());
                }

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
                model,
                final_usage.as_ref().map(|u| u.total_tokens).unwrap_or(0)
            );

            // Add final assistant response to storage
            storage
                .add_message(StorageMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: session_id.to_string(),
                    role: "assistant".to_string(),
                    content: content_buf,
                    created_at: Utc::now(),
                    model_used: Some(model.clone()),
                    tokens: final_usage.as_ref().map(|u| u.total_tokens),
                })
                .await?;

            // Send done event
            if tx
                .send(StreamEvent::Done {
                    model,
                    usage: final_usage,
                })
                .await
                .is_err()
            {
                // Receiver dropped - client disconnected
            }

            break;
        }
    }

    Ok(())
}
