use super::{
    CacheManager, ChatMessage, ChatRequest, ChatResponse, ModelRouter, StreamChunk, TokenUsage,
    ToolCall, ToolCallChunk,
};
use crate::config::LlmConfig;
use anyhow::{Context, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs,
    },
    Client as OpenAIClient,
};
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

/// LLM client with hot-swapping support
#[derive(Clone)]
pub struct Client {
    client: OpenAIClient<OpenAIConfig>,
    config: LlmConfig,
    cache_manager: Arc<Mutex<CacheManager>>,
    router: Arc<ModelRouter>,
}

impl Client {
    pub fn new(config: &LlmConfig) -> Result<Self> {
        let openai_config = OpenAIConfig::new().with_api_base(&config.base_url);
        let client = OpenAIClient::with_config(openai_config);

        let cache_manager = CacheManager::new(config);
        let router = ModelRouter::new(config)?;

        Ok(Self {
            client,
            config: config.clone(),
            cache_manager: Arc::new(Mutex::new(cache_manager)),
            router: Arc::new(router),
        })
    }

    /// Send chat request with automatic model routing and hot-swapping
    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        // Determine which model to use via routing
        let model = if request.model.is_empty() {
            // Auto-route based on last user message
            let last_message = request
                .messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .map(|m| m.content.as_str())
                .unwrap_or("");

            self.router.route(last_message).to_string()
        } else {
            // Use explicitly specified model
            request.model.clone()
        };

        // Get keep_alive from cache strategy
        let keep_alive = {
            let cache = self.cache_manager.lock().await;
            cache.keep_alive()
        };

        tracing::info!(
            "Sending request to LLM: model={}, keep_alive={}, messages={}",
            model,
            keep_alive,
            request.messages.len()
        );

        // Convert messages
        let messages: Result<Vec<ChatCompletionRequestMessage>> = request
            .messages
            .iter()
            .map(|msg| self.convert_message(msg))
            .collect();
        let messages = messages?;

        // Build request
        let mut req_builder = CreateChatCompletionRequestArgs::default();
        req_builder.model(&model);
        req_builder.messages(messages);

        if let Some(max_tokens) = request.max_tokens {
            req_builder.max_tokens(max_tokens as u16);
        }

        if let Some(temperature) = request.temperature {
            req_builder.temperature(temperature);
        }

        // Add tools if provided
        if let Some(tools) = request.tools {
            // Convert our ToolDefinition to OpenAI format
            let openai_tools: Vec<serde_json::Value> = tools
                .into_iter()
                .map(|tool| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters,
                        }
                    })
                })
                .collect();

            if !openai_tools.is_empty() {
                let converted_tools: Vec<async_openai::types::ChatCompletionTool> = openai_tools
                    .into_iter()
                    .filter_map(|tool| serde_json::from_value(tool).ok())
                    .collect();

                if !converted_tools.is_empty() {
                    req_builder.tools(converted_tools);
                    // Use auto tool choice to let the model decide
                    req_builder
                        .tool_choice(async_openai::types::ChatCompletionToolChoiceOption::Auto);
                }
            }
        }

        let req = req_builder
            .build()
            .context("Failed to build chat completion request")?;

        // Send request to Ollama/LLM backend
        let response = self
            .client
            .chat()
            .create(req)
            .await
            .context("Failed to get chat completion")?;

        let choice = response
            .choices
            .first()
            .context("No choices in chat completion response")?;

        let content = choice.message.content.clone().unwrap_or_default();

        // Extract tool calls if present
        let tool_calls = choice.message.tool_calls.as_ref().map(|calls| {
            calls
                .iter()
                .map(|call| ToolCall {
                    id: call.id.clone(),
                    name: call.function.name.clone(),
                    arguments: call.function.arguments.clone(),
                })
                .collect::<Vec<ToolCall>>()
        });

        // Extract token usage if available
        let usage = response.usage.as_ref().map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens as usize,
            completion_tokens: u.completion_tokens as usize,
            total_tokens: u.total_tokens as usize,
        });

        // Mark model as used in cache
        {
            let mut cache = self.cache_manager.lock().await;
            cache.mark_used(&model);
        }

        // Log response details
        if let Some(ref calls) = tool_calls {
            if !calls.is_empty() {
                let tool_names: Vec<&str> = calls.iter().map(|t| t.name.as_str()).collect();
                tracing::info!(
                    "Received response with tool calls: model={}, tools={:?}",
                    model,
                    tool_names
                );
            }
        } else {
            tracing::info!(
                "Received response from LLM: model={}, tokens={:?}",
                model,
                usage
            );
        }

        Ok(ChatResponse {
            content,
            model: response.model,
            finish_reason: choice.finish_reason.as_ref().map(|r| format!("{:?}", r)),
            usage,
            tool_calls,
        })
    }

    pub fn primary_model(&self) -> &str {
        &self.config.models.primary
    }



    /// Route a message to the appropriate model based on content
    pub fn route_model(&self, content: &str) -> &str {
        self.router.route(content)
    }

    /// Stream chat completion (for streaming responses)
    pub async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        // Determine which model to use via routing
        let model = if request.model.is_empty() {
            // Auto-route based on last user message
            let last_message = request
                .messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .map(|m| m.content.as_str())
                .unwrap_or("");

            self.router.route(last_message).to_string()
        } else {
            // Use explicitly specified model
            request.model.clone()
        };

        // Get keep_alive from cache strategy
        let keep_alive = {
            let cache = self.cache_manager.lock().await;
            cache.keep_alive()
        };

        tracing::info!(
            "Streaming request to LLM: model={}, keep_alive={}, messages={}",
            model,
            keep_alive,
            request.messages.len()
        );

        // Convert messages
        let messages: Result<Vec<ChatCompletionRequestMessage>> = request
            .messages
            .iter()
            .map(|msg| self.convert_message(msg))
            .collect();
        let messages = messages?;

        // Build request
        let mut req_builder = CreateChatCompletionRequestArgs::default();
        req_builder.model(&model);
        req_builder.messages(messages);

        if let Some(max_tokens) = request.max_tokens {
            req_builder.max_tokens(max_tokens as u16);
        }

        if let Some(temperature) = request.temperature {
            req_builder.temperature(temperature);
        }

        // Add tools if provided
        if let Some(tools) = request.tools {
            // Convert our ToolDefinition to OpenAI format
            let openai_tools: Vec<serde_json::Value> = tools
                .into_iter()
                .map(|tool| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters,
                        }
                    })
                })
                .collect();

            if !openai_tools.is_empty() {
                let converted_tools: Vec<async_openai::types::ChatCompletionTool> = openai_tools
                    .into_iter()
                    .filter_map(|tool| serde_json::from_value(tool).ok())
                    .collect();

                if !converted_tools.is_empty() {
                    req_builder.tools(converted_tools);
                    req_builder
                        .tool_choice(async_openai::types::ChatCompletionToolChoiceOption::Auto);
                }
            }
        }

        let req = req_builder
            .build()
            .context("Failed to build chat completion request")?;

        // Send streaming request to Ollama/LLM backend
        let stream = self
            .client
            .chat()
            .create_stream(req)
            .await
            .context("Failed to create chat stream")?;

        // Mark model as used in cache
        {
            let mut cache = self.cache_manager.lock().await;
            cache.mark_used(&model);
        }

        // Convert stream items to our StreamChunk type
        let model_clone = model.clone();
        let mapped_stream = stream.map(move |result| {
            result.context("Stream error").map(|response| {
                let choice = response.choices.first();
                let content = choice
                    .and_then(|c| c.delta.content.clone())
                    .filter(|s| !s.is_empty());

                let tool_calls = choice
                    .and_then(|c| c.delta.tool_calls.as_ref())
                    .map(|calls| {
                        calls
                            .iter()
                            .map(|tc| ToolCallChunk {
                                index: tc.index as usize,
                                id: tc.id.clone(),
                                name: tc.function.as_ref().and_then(|f| f.name.clone()),
                                arguments: tc.function.as_ref().and_then(|f| f.arguments.clone()),
                            })
                            .collect()
                    });

                let finish_reason = choice
                    .and_then(|c| c.finish_reason.as_ref())
                    .map(|r| format!("{:?}", r));

                StreamChunk {
                    content,
                    tool_calls: tool_calls.filter(|tc: &Vec<ToolCallChunk>| !tc.is_empty()),
                    finish_reason,
                    model: Some(model_clone.clone()),
                    usage: None, // Stream responses don't include usage info
                }
            })
        });

        Ok(Box::pin(mapped_stream))
    }

    fn convert_message(&self, msg: &ChatMessage) -> Result<ChatCompletionRequestMessage> {
        match msg.role.as_str() {
            "system" => Ok(ChatCompletionRequestSystemMessageArgs::default()
                .content(msg.content.clone())
                .build()?
                .into()),
            "user" => Ok(ChatCompletionRequestUserMessageArgs::default()
                .content(msg.content.clone())
                .build()?
                .into()),
            "assistant" => Ok(ChatCompletionRequestAssistantMessageArgs::default()
                .content(msg.content.clone())
                .build()?
                .into()),
            _ => anyhow::bail!("Unknown message role: {}", msg.role),
        }
    }
}
