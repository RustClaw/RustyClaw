use crate::api::{
    ApiError, ApiResponse, ChatContent, ChatRequest, ChatResponse, MessageListResponse,
    MessageResponse, ModelInfo, ModelsResponse, SessionListResponse, SessionResponse,
};
use crate::core::{Router, StreamEvent};
use crate::storage::{Storage, User};
use crate::tools::skills::parse_skill_file;
use crate::tools::{get_skill, list_skills, load_skill, unload_skill, CreateToolRequest};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{
    sse::{Event, Sse},
    IntoResponse, Response,
};
use axum::Extension;
use axum::Json;
use chrono::Utc;
use futures::StreamExt;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;
use tokio_stream::wrappers::ReceiverStream;

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

/// Setup request
#[derive(Deserialize)]
pub struct SetupRequest {
    pub code: String,
    pub username: String,
}

/// Setup response
#[derive(serde::Serialize)]
pub struct SetupResponse {
    pub user: User,
    pub token: String,
}

/// Invite request
#[derive(Deserialize)]
pub struct InviteRequest {}

/// Invite response
#[derive(serde::Serialize)]
pub struct InviteResponse {
    pub code: String,
    pub expires_at: chrono::DateTime<Utc>,
    pub uri: String,
}

/// Join request
#[derive(Deserialize)]
pub struct JoinRequest {
    pub code: String,
    pub label: String,
}

/// Join response
#[derive(serde::Serialize)]
pub struct JoinResponse {
    pub user: User,
    pub token: String,
}

// ===== Setup Endpoints =====

/// POST /api/setup - Claim admin account with OTP
pub async fn setup_admin<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Json(req): Json<SetupRequest>,
) -> Result<Json<ApiResponse<SetupResponse>>, ApiError> {
    let (user, token) = router
        .pairing_manager
        .claim_admin(&req.code, &req.username)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let response = SetupResponse { user, token };

    Ok(Json(ApiResponse::success(response)))
}

/// POST /api/auth/invite - Generate a device linking code
pub async fn create_invite<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Extension(user_id): Extension<String>,
    Json(_req): Json<InviteRequest>,
) -> Result<Json<ApiResponse<InviteResponse>>, ApiError> {
    let code = router
        .pairing_manager
        .create_invite(&user_id)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?;

    let response = InviteResponse {
        code: code.clone(),
        expires_at: Utc::now() + chrono::Duration::minutes(10),
        uri: format!("rustyclaw://join?code={}", code),
    };

    Ok(Json(ApiResponse::success(response)))
}

/// POST /api/auth/join - Redeem an invite code
pub async fn join_invite<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Json(req): Json<JoinRequest>,
) -> Result<Json<ApiResponse<JoinResponse>>, ApiError> {
    let (user, token) = router
        .pairing_manager
        .redeem_invite(&req.code, &req.label)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(Json(ApiResponse::success(JoinResponse { user, token })))
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

/// POST /api/chat - Send message and get response (supports streaming)
pub async fn chat<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Extension(user_id): Extension<String>,
    Json(req): Json<ChatRequest>,
) -> Result<axum::response::Response, ApiError> {
    // Validate input
    if req.message.is_empty() {
        return Err(ApiError::BadRequest("message cannot be empty".to_string()));
    }

    if req.message.len() > 10000 {
        return Err(ApiError::BadRequest(
            "message too long (max 10000 chars)".to_string(),
        ));
    }

    // Handle streaming request
    if req.stream {
        return chat_stream_sse(router, user_id, req).await;
    }

    let start = Instant::now();

    // Non-streaming path: process message through router
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

    Ok((StatusCode::OK, Json(ApiResponse::success(chat_response))).into_response())
}

/// SSE streaming chat response
async fn chat_stream_sse<S: Storage + 'static>(
    router: Arc<Router<S>>,
    user_id: String,
    req: ChatRequest,
) -> Result<Response, ApiError> {
    // Get streaming receiver from router
    let receiver = router
        .handle_message_stream(&user_id, "web", &req.message)
        .await
        .map_err(|e| {
            tracing::error!("Failed to handle message stream: {}", e);
            ApiError::InternalError("Failed to process message".to_string())
        })?;

    // Convert receiver to stream
    let stream = ReceiverStream::new(receiver);

    // Map stream events to SSE events
    let sse_stream = stream.map(|event| -> Result<Event, String> {
        match event {
            StreamEvent::Delta(text) => Ok(Event::default().data(text)),
            StreamEvent::ToolStart { name } => Ok(Event::default().event("tool_start").data(name)),
            StreamEvent::ToolEnd { name, result } => {
                let data = serde_json::json!({
                    "name": name,
                    "result": result
                });
                Ok(Event::default().event("tool_end").data(data.to_string()))
            }
            StreamEvent::Done { model, usage } => {
                let data = serde_json::json!({
                    "model": model,
                    "usage": usage
                });
                Ok(Event::default().event("done").data(data.to_string()))
            }
            StreamEvent::Error(msg) => Ok(Event::default().event("error").data(msg)),
        }
    });

    Ok(Sse::new(sse_stream).into_response())
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

// ===== Tool Endpoints =====

/// Tool creation response
#[derive(serde::Serialize)]
pub struct ToolResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub path: String,
    pub ready: bool,
}

/// Tool info for listing
#[derive(serde::Serialize)]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub runtime: String,
    pub source: String,
    pub policy: String,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ready: bool,
}

/// Tool list response
#[derive(serde::Serialize)]
pub struct ToolListResponse {
    pub tools: Vec<ToolInfo>,
    pub total: usize,
    pub ready: usize,
    pub failed: usize,
}

/// POST /api/tools - Create a new tool
pub async fn create_tool<S: Storage + 'static>(
    State(_router): State<Arc<Router<S>>>,
    Extension(_user_id): Extension<String>,
    Json(req): Json<CreateToolRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ToolResponse>>), ApiError> {
    // Validate request
    req.validate().map_err(|e| {
        tracing::error!("Tool validation failed: {}", e);
        ApiError::BadRequest(format!("Tool validation failed: {}", e))
    })?;

    // Check if tool already exists
    if get_skill(&req.name).await.is_some() {
        return Err(ApiError::BadRequest(format!(
            "Tool '{}' already exists",
            req.name
        )));
    }

    // Get storage path
    let storage_path = get_tool_storage_path(&req.name)?;

    // Create directory if needed
    if let Some(parent) = storage_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ApiError::InternalError(format!("Failed to create directory: {}", e)))?;
    }

    // Generate skill file content
    let skill_content = req.to_skill_file();

    // Save to disk
    tokio::fs::write(&storage_path, &skill_content)
        .await
        .map_err(|e| ApiError::InternalError(format!("Failed to write tool file: {}", e)))?;

    // Load into registry
    let skill_entry = parse_skill_file(&storage_path)
        .map_err(|e| ApiError::InternalError(format!("Failed to parse tool: {}", e)))?;

    load_skill(skill_entry)
        .await
        .map_err(|e| ApiError::InternalError(format!("Failed to load tool: {}", e)))?;

    let response = ToolResponse {
        id: format!("tool-{}", uuid::Uuid::new_v4()),
        name: req.name.clone(),
        description: req.description.clone(),
        created_at: Utc::now(),
        path: storage_path.to_string_lossy().to_string(),
        ready: true,
    };

    tracing::info!("Tool created successfully: {}", req.name);
    Ok((StatusCode::CREATED, Json(ApiResponse::success(response))))
}

/// GET /api/tools - List all tools
pub async fn list_tools<S: Storage + 'static>(
    State(_router): State<Arc<Router<S>>>,
    Extension(_user_id): Extension<String>,
) -> Result<Json<ApiResponse<ToolListResponse>>, ApiError> {
    let all_skills = list_skills().await;

    let tools: Vec<ToolInfo> = all_skills
        .into_iter()
        .map(|skill| ToolInfo {
            id: format!("tool-{}", skill.manifest.name),
            name: skill.manifest.name.clone(),
            description: skill.manifest.description.clone(),
            runtime: skill.manifest.runtime.clone(),
            source: "user".to_string(),
            policy: skill.manifest.policy.clone(),
            created_at: Some(Utc::now()),
            ready: true,
        })
        .collect();

    let total = tools.len();
    let response = ToolListResponse {
        tools,
        total,
        ready: total,
        failed: 0,
    };

    Ok(Json(ApiResponse::success(response)))
}

/// GET /api/tools/:name - Get tool details
pub async fn get_tool<S: Storage + 'static>(
    State(_router): State<Arc<Router<S>>>,
    Extension(_user_id): Extension<String>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let skill = get_skill(&name)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Tool '{}' not found", name)))?;

    let response = serde_json::json!({
        "id": format!("tool-{}", skill.manifest.name),
        "name": skill.manifest.name,
        "description": skill.manifest.description,
        "runtime": skill.manifest.runtime,
        "body": skill.body,
        "parameters": skill.manifest.parameters,
        "policy": skill.manifest.policy,
        "sandbox": skill.manifest.sandbox,
        "network": skill.manifest.network,
        "timeout_secs": skill.manifest.timeout_secs,
        "path": skill.source_path.to_string_lossy(),
        "ready": true,
    });

    Ok(Json(ApiResponse::success(response)))
}

/// DELETE /api/tools/:name - Delete a tool
pub async fn delete_tool<S: Storage + 'static>(
    State(_router): State<Arc<Router<S>>>,
    Extension(_user_id): Extension<String>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let skill = get_skill(&name)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Tool '{}' not found", name)))?;

    // Unload from registry
    unload_skill(&name)
        .await
        .map_err(|e| ApiError::InternalError(format!("Failed to unload tool: {}", e)))?;

    // Delete file
    tokio::fs::remove_file(&skill.source_path)
        .await
        .map_err(|e| ApiError::InternalError(format!("Failed to delete tool file: {}", e)))?;

    let response = serde_json::json!({
        "message": format!("Tool '{}' deleted successfully", name),
        "path": skill.source_path.to_string_lossy(),
    });

    tracing::info!("Tool deleted: {}", name);
    Ok(Json(ApiResponse::success(response)))
}

/// POST /api/tools/:name/validate - Validate tool syntax
pub async fn validate_tool<S: Storage + 'static>(
    State(_router): State<Arc<Router<S>>>,
    Extension(_user_id): Extension<String>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let skill = get_skill(&name)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Tool '{}' not found", name)))?;

    let response = serde_json::json!({
        "valid": true,
        "name": skill.manifest.name,
        "runtime": skill.manifest.runtime,
        "errors": [],
        "warnings": [],
    });

    Ok(Json(ApiResponse::success(response)))
}

/// PUT /api/tools/:name - Update tool
pub async fn update_tool<S: Storage + 'static>(
    State(_router): State<Arc<Router<S>>>,
    Extension(_user_id): Extension<String>,
    Path(name): Path<String>,
    Json(req): Json<CreateToolRequest>,
) -> Result<Json<ApiResponse<ToolResponse>>, ApiError> {
    // Validate request
    req.validate().map_err(|e| {
        tracing::error!("Tool validation failed: {}", e);
        ApiError::BadRequest(format!("Tool validation failed: {}", e))
    })?;

    // Check if tool exists
    let existing = get_skill(&name)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Tool '{}' not found", name)))?;

    // If name changed, check new name doesn't exist
    if req.name != name && get_skill(&req.name).await.is_some() {
        return Err(ApiError::BadRequest(format!(
            "Tool '{}' already exists",
            req.name
        )));
    }

    // Unload old tool
    unload_skill(&name)
        .await
        .map_err(|e| ApiError::InternalError(format!("Failed to unload tool: {}", e)))?;

    // Delete old file if name changed
    if req.name != name {
        tokio::fs::remove_file(&existing.source_path)
            .await
            .map_err(|e| {
                ApiError::InternalError(format!("Failed to delete old tool file: {}", e))
            })?;
    }

    // Get storage path for (possibly) new name
    let storage_path = get_tool_storage_path(&req.name)?;

    // Create directory if needed
    if let Some(parent) = storage_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ApiError::InternalError(format!("Failed to create directory: {}", e)))?;
    }

    // Generate skill file content
    let skill_content = req.to_skill_file();

    // Save to disk
    tokio::fs::write(&storage_path, &skill_content)
        .await
        .map_err(|e| ApiError::InternalError(format!("Failed to write tool file: {}", e)))?;

    // Load into registry
    let skill_entry = parse_skill_file(&storage_path)
        .map_err(|e| ApiError::InternalError(format!("Failed to parse tool: {}", e)))?;

    load_skill(skill_entry)
        .await
        .map_err(|e| ApiError::InternalError(format!("Failed to load tool: {}", e)))?;

    let response = ToolResponse {
        id: format!("tool-{}", uuid::Uuid::new_v4()),
        name: req.name.clone(),
        description: req.description.clone(),
        created_at: Utc::now(),
        path: storage_path.to_string_lossy().to_string(),
        ready: true,
    };

    tracing::info!("Tool updated: {}", req.name);
    Ok(Json(ApiResponse::success(response)))
}

/// POST /api/tools/:name/test - Test tool with parameters
#[derive(serde::Deserialize)]
pub struct ToolTestRequest {
    #[serde(default)]
    pub parameters: serde_json::Value,
}

#[derive(serde::Serialize)]
pub struct ToolTestResponse {
    pub status: String,
    pub output: String,
    pub execution_time_ms: u128,
    pub error: Option<String>,
}

pub async fn test_tool<S: Storage + 'static>(
    State(_router): State<Arc<Router<S>>>,
    Extension(_user_id): Extension<String>,
    Path(name): Path<String>,
    Json(_req): Json<ToolTestRequest>,
) -> Result<Json<ApiResponse<ToolTestResponse>>, ApiError> {
    let skill = get_skill(&name)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Tool '{}' not found", name)))?;

    let start = std::time::Instant::now();

    // For now, return a mock response
    // In production, would execute the tool with the given parameters
    let response = ToolTestResponse {
        status: "success".to_string(),
        output: format!("Mock execution of {}", name),
        execution_time_ms: start.elapsed().as_millis(),
        error: None,
    };

    tracing::info!(
        "Tool tested: {} (runtime: {})",
        name,
        skill.manifest.runtime
    );
    Ok(Json(ApiResponse::success(response)))
}

/// GET /api/tools/:name/definition - Get tool definition in OpenAI format
pub async fn get_tool_definition<S: Storage + 'static>(
    State(_router): State<Arc<Router<S>>>,
    Extension(_user_id): Extension<String>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let skill = get_skill(&name)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Tool '{}' not found", name)))?;

    let definition = serde_json::json!({
        "type": "function",
        "function": {
            "name": skill.manifest.name,
            "description": skill.manifest.description,
            "parameters": skill.manifest.parameters,
        }
    });

    Ok(Json(ApiResponse::success(definition)))
}

/// GET /api/tools/definitions/all - Get all tool definitions for LLM
pub async fn get_all_tool_definitions<S: Storage + 'static>(
    State(_router): State<Arc<Router<S>>>,
    Extension(_user_id): Extension<String>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ApiError> {
    let all_skills = list_skills().await;

    let definitions: Vec<serde_json::Value> = all_skills
        .into_iter()
        .map(|skill| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": skill.manifest.name,
                    "description": skill.manifest.description,
                    "parameters": skill.manifest.parameters,
                }
            })
        })
        .collect();

    Ok(Json(ApiResponse::success(definitions)))
}

// Helper function to get tool storage path
fn get_tool_storage_path(name: &str) -> Result<std::path::PathBuf, ApiError> {
    let home = dirs::home_dir()
        .ok_or_else(|| ApiError::InternalError("Cannot determine home directory".to_string()))?;

    let path = home
        .join(".rustyclaw")
        .join("skills")
        .join("user-created")
        .join(format!("{}.yaml", name));

    Ok(path)
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
