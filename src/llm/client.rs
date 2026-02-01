use super::{ChatMessage, ChatRequest, ChatResponse};
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

#[derive(Clone)]
pub struct Client {
    client: OpenAIClient<OpenAIConfig>,
    config: LlmConfig,
}

impl Client {
    pub fn new(config: &LlmConfig) -> Result<Self> {
        let openai_config = OpenAIConfig::new().with_api_base(&config.base_url);

        let client = OpenAIClient::with_config(openai_config);

        Ok(Self {
            client,
            config: config.clone(),
        })
    }

    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let messages: Result<Vec<ChatCompletionRequestMessage>> = request
            .messages
            .iter()
            .map(|msg| self.convert_message(msg))
            .collect();

        let messages = messages?;

        let mut req_builder = CreateChatCompletionRequestArgs::default();
        req_builder.model(&request.model);
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

        Ok(ChatResponse {
            content,
            model: response.model,
            finish_reason: choice.finish_reason.as_ref().map(|r| format!("{:?}", r)),
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
