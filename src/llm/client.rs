use super::{CacheManager, ChatMessage, ChatRequest, ChatResponse, ModelRouter, TokenUsage};
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

        tracing::info!(
            "Received response from LLM: model={}, tokens={:?}",
            model,
            usage
        );

        Ok(ChatResponse {
            content,
            model: response.model,
            finish_reason: choice.finish_reason.as_ref().map(|r| format!("{:?}", r)),
            usage,
        })
    }

    pub fn primary_model(&self) -> &str {
        &self.config.models.primary
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
