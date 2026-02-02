use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Standard API response wrapper for success responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            data: Some(data),
            status: "success".to_string(),
            timestamp: Some(Utc::now()),
        }
    }

    pub fn ok() -> ApiResponse<()> {
        ApiResponse {
            data: Some(()),
            status: "success".to_string(),
            timestamp: Some(Utc::now()),
        }
    }
}

/// Session response object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    pub id: String,
    pub user_id: String,
    pub channel: String,
    pub scope: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    pub tokens_used: usize,
    pub context_window: usize,
    pub status: String,
}

/// Message response object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageResponse {
    pub id: String,
    pub session_id: String,
    pub user_id: String,
    pub channel: String,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub tokens: Option<usize>,
    pub model_used: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Chat request
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub stream: bool,
}

/// Chat response
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub status: String,
    pub message_id: String,
    pub session_id: String,
    pub user_id: String,
    pub timestamp: DateTime<Utc>,
    pub input: ChatContent,
    pub response: ChatContent,
    pub latency_ms: u64,
}

/// Chat message content
#[derive(Debug, Serialize)]
pub struct ChatContent {
    pub text: String,
    pub tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Session list response
#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionResponse>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Message list response
#[derive(Debug, Serialize)]
pub struct MessageListResponse {
    pub session_id: String,
    pub messages: Vec<MessageResponse>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Model info response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub role: String,
    pub vram_mb: usize,
    pub loaded: bool,
}

/// Models list response
#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub models: Vec<ModelInfo>,
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub gateway: String,
}

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSocketMessage {
    /// Client → Server: Send message
    Message { content: String },

    /// Server → Client: Connection established
    Connected { session_id: String },

    /// Server → Client: Response started
    Start {
        session_id: String,
        message_id: String,
    },

    /// Server → Client: Response chunk
    Stream { content: String },

    /// Server → Client: Response completed
    End {
        message_id: String,
        total_tokens: usize,
        model: String,
        latency_ms: u64,
    },

    /// Server → Client: Error occurred
    Error { error: String, error_code: u32 },

    /// Server → Client: Keepalive ping
    Ping,

    /// Client → Server: Keepalive pong
    Pong,
}

impl WebSocketMessage {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_success() {
        let response: ApiResponse<&str> = ApiResponse::success("test");
        assert_eq!(response.status, "success");
        assert_eq!(response.data, Some("test"));
        assert!(response.timestamp.is_some());
    }

    #[test]
    fn test_websocket_message_serialization() {
        let msg = WebSocketMessage::Message {
            content: "hello".to_string(),
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("hello"));

        let parsed = WebSocketMessage::from_json(&json).unwrap();
        if let WebSocketMessage::Message { content } = parsed {
            assert_eq!(content, "hello");
        } else {
            panic!("Wrong message type");
        }
    }

    #[test]
    fn test_websocket_ping_pong() {
        let ping = WebSocketMessage::Ping;
        let pong = WebSocketMessage::Pong;

        assert!(ping.to_json().is_ok());
        assert!(pong.to_json().is_ok());
    }
}
