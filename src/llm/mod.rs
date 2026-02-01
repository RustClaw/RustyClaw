mod client;

pub use client::Client;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub finish_reason: Option<String>,
}
