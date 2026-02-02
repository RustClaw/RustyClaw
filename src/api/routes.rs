use crate::api::{
    ApiError, ApiResponse, ChatContent, ChatRequest, ChatResponse, MessageListResponse,
    MessageResponse, ModelInfo, ModelsResponse, SessionListResponse, SessionResponse,
};
use crate::core::Router;
use crate::storage::Storage;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;

/// Query parameters for listing messages
#[derive(Deserialize)]
pub struct MessageQuery {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Create session request
#[derive(Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub scope: Option<String>,
}

// ===== Session Endpoints =====

/// POST /api/sessions - Create a new session
pub async fn create_session<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Extension(user_id): Extension<String>,
    Json(_req): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<ApiResponse<SessionResponse>>), ApiError> {
    let session = router
        .get_or_create_session_api(&user_id, "web")
        .await
        .map_err(|e| {
            tracing::error!("Failed to create session: {}", e);
            ApiError::InternalError("Failed to create session".to_string())
        })?;

    let stats = router
        .get_session_stats(&user_id, "web")
        .await
        .map_err(|e| {
            tracing::error!("Failed to get session stats: {}", e);
            ApiError::InternalError("Failed to get session stats".to_string())
        })?;

    let response = SessionResponse {
        id: session.id,
        user_id: session.user_id,
        channel: session.channel,
        scope: "per-sender".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        message_count: stats.total_messages,
        tokens_used: stats.total_tokens,
        context_window: 128000,
        status: "active".to_string(),
    };

    Ok((StatusCode::CREATED, Json(ApiResponse::success(response))))
}

/// GET /api/sessions - List user's sessions
pub async fn list_sessions<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Extension(user_id): Extension<String>,
) -> Result<Json<ApiResponse<SessionListResponse>>, ApiError> {
    // For now, return a single session per user (per-sender scope)
    let session = router
        .get_or_create_session_api(&user_id, "web")
        .await
        .map_err(|e| {
            tracing::error!("Failed to get session: {}", e);
            ApiError::InternalError("Failed to get session".to_string())
        })?;

    let stats = router
        .get_session_stats(&user_id, "web")
        .await
        .map_err(|e| {
            tracing::error!("Failed to get session stats: {}", e);
            ApiError::InternalError("Failed to get session stats".to_string())
        })?;

    let session_response = SessionResponse {
        id: session.id,
        user_id: session.user_id,
        channel: session.channel,
        scope: "per-sender".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        message_count: stats.total_messages,
        tokens_used: stats.total_tokens,
        context_window: 128000,
        status: "active".to_string(),
    };

    let response = SessionListResponse {
        sessions: vec![session_response],
        total: 1,
        limit: 100,
        offset: 0,
    };

    Ok(Json(ApiResponse::success(response)))
}

/// GET /api/sessions/:id - Get session details
pub async fn get_session<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Extension(user_id): Extension<String>,
    Path(_session_id): Path<String>,
) -> Result<Json<ApiResponse<SessionResponse>>, ApiError> {
    let session = router
        .get_or_create_session_api(&user_id, "web")
        .await
        .map_err(|e| {
            tracing::error!("Failed to get session: {}", e);
            ApiError::InternalError("Failed to get session".to_string())
        })?;

    let stats = router
        .get_session_stats(&user_id, "web")
        .await
        .map_err(|e| {
            tracing::error!("Failed to get session stats: {}", e);
            ApiError::InternalError("Failed to get session stats".to_string())
        })?;

    let response = SessionResponse {
        id: session.id,
        user_id: session.user_id,
        channel: session.channel,
        scope: "per-sender".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        message_count: stats.total_messages,
        tokens_used: stats.total_tokens,
        context_window: 128000,
        status: "active".to_string(),
    };

    Ok(Json(ApiResponse::success(response)))
}

/// DELETE /api/sessions/:id - Delete session
pub async fn delete_session<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Extension(user_id): Extension<String>,
    Path(_session_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    router.clear_session(&user_id, "web").await.map_err(|e| {
        tracing::error!("Failed to clear session: {}", e);
        ApiError::InternalError("Failed to clear session".to_string())
    })?;

    Ok(StatusCode::NO_CONTENT)
}

// ===== Chat Endpoints =====

/// POST /api/chat - Send message and get response
pub async fn chat<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Extension(user_id): Extension<String>,
    Json(req): Json<ChatRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ChatResponse>>), ApiError> {
    // Validate input
    if req.message.is_empty() {
        return Err(ApiError::BadRequest("message cannot be empty".to_string()));
    }

    if req.message.len() > 10000 {
        return Err(ApiError::BadRequest(
            "message too long (max 10000 chars)".to_string(),
        ));
    }

    let start = Instant::now();

    // Process message through router
    let response = router
        .handle_message(&user_id, "web", &req.message)
        .await
        .map_err(|e| {
            tracing::error!("Failed to handle message: {}", e);
            match e.downcast_ref::<std::string::String>() {
                Some(msg) if msg.contains("unavailable") => {
                    ApiError::ServiceUnavailable("LLM service unavailable".to_string())
                }
                _ => ApiError::InternalError("Failed to process message".to_string()),
            }
        })?;

    let latency_ms = start.elapsed().as_millis() as u64;

    // Get session for response
    let session = router
        .get_or_create_session_api(&user_id, "web")
        .await
        .map_err(|e| {
            tracing::error!("Failed to get session: {}", e);
            ApiError::InternalError("Failed to get session".to_string())
        })?;

    let chat_response = ChatResponse {
        status: "success".to_string(),
        message_id: format!("msg-{}", uuid::Uuid::new_v4()),
        session_id: session.id,
        user_id,
        timestamp: Utc::now(),
        input: ChatContent {
            text: req.message,
            tokens: 0, // TODO: Calculate token count
            model: None,
        },
        response: ChatContent {
            text: response.content.clone(),
            tokens: response.tokens.unwrap_or(0),
            model: Some(response.model),
        },
        latency_ms,
    };

    Ok((StatusCode::OK, Json(ApiResponse::success(chat_response))))
}

// ===== Message Endpoints =====

/// GET /api/messages - Get conversation history
pub async fn list_messages<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Extension(user_id): Extension<String>,
    Query(params): Query<MessageQuery>,
) -> Result<Json<ApiResponse<MessageListResponse>>, ApiError> {
    let session = router
        .get_or_create_session_api(&user_id, "web")
        .await
        .map_err(|e| {
            tracing::error!("Failed to get session: {}", e);
            ApiError::InternalError("Failed to get session".to_string())
        })?;

    let limit = params.limit.unwrap_or(50).min(500); // Max 500
    let offset = params.offset.unwrap_or(0);

    let messages = router
        .get_session_messages(&session.id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get messages: {}", e);
            ApiError::InternalError("Failed to get messages".to_string())
        })?;

    let total = messages.len();
    let paginated = messages
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|msg| MessageResponse {
            id: msg.id,
            session_id: msg.session_id,
            user_id: user_id.clone(),
            channel: "web".to_string(),
            role: msg.role,
            content: msg.content,
            timestamp: msg.created_at,
            tokens: msg.tokens,
            model_used: msg.model_used,
            metadata: None,
        })
        .collect();

    let response = MessageListResponse {
        session_id: session.id,
        messages: paginated,
        total,
        limit,
        offset,
    };

    Ok(Json(ApiResponse::success(response)))
}

/// GET /api/messages/:id - Get single message
pub async fn get_message<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Extension(user_id): Extension<String>,
    Path(_message_id): Path<String>,
) -> Result<Json<ApiResponse<MessageResponse>>, ApiError> {
    let session = router
        .get_or_create_session_api(&user_id, "web")
        .await
        .map_err(|e| {
            tracing::error!("Failed to get session: {}", e);
            ApiError::InternalError("Failed to get session".to_string())
        })?;

    let messages = router
        .get_session_messages(&session.id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get messages: {}", e);
            ApiError::InternalError("Failed to get messages".to_string())
        })?;

    // For now, return first message (in production, filter by ID)
    let msg = messages
        .first()
        .ok_or_else(|| ApiError::NotFound("Message not found".to_string()))?;

    let response = MessageResponse {
        id: msg.id.clone(),
        session_id: msg.session_id.clone(),
        user_id,
        channel: "web".to_string(),
        role: msg.role.clone(),
        content: msg.content.clone(),
        timestamp: msg.created_at,
        tokens: msg.tokens,
        model_used: msg.model_used.clone(),
        metadata: None,
    };

    Ok(Json(ApiResponse::success(response)))
}

// ===== Models Endpoints =====

/// GET /api/models - List available models
pub async fn list_models<S: Storage + 'static>(
    State(_router): State<Arc<Router<S>>>,
    Extension(_user_id): Extension<String>,
) -> Result<Json<ApiResponse<ModelsResponse>>, ApiError> {
    // Return hardcoded models (in production, get from LLM client)
    let models = vec![
        ModelInfo {
            name: "qwen2.5:32b".to_string(),
            role: "primary".to_string(),
            vram_mb: 14000,
            loaded: true,
        },
        ModelInfo {
            name: "deepseek-coder-v2:16b".to_string(),
            role: "code".to_string(),
            vram_mb: 9000,
            loaded: false,
        },
        ModelInfo {
            name: "qwen2.5:7b".to_string(),
            role: "fast".to_string(),
            vram_mb: 5000,
            loaded: true,
        },
    ];

    Ok(Json(ApiResponse::success(ModelsResponse { models })))
}

/// POST /api/models/:name/load - Load model
pub async fn load_model<S: Storage + 'static>(
    State(_router): State<Arc<Router<S>>>,
    Extension(_user_id): Extension<String>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    // In production, call LLM client to load model
    // For now, just acknowledge the request
    let response = serde_json::json!({
        "model": name,
        "status": "loading",
        "vram_mb": 14000
    });

    Ok(Json(ApiResponse::success(response)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_query_defaults() {
        let query = MessageQuery {
            limit: None,
            offset: None,
        };

        assert_eq!(query.limit, None);
        assert_eq!(query.offset, None);
    }

    #[test]
    fn test_create_session_request() {
        let req = CreateSessionRequest { scope: None };
        assert_eq!(req.scope, None);
    }
}
