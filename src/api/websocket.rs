use crate::api::{ApiError, AuthManager, WebSocketMessage};
use crate::core::{Router, StreamEvent};
use crate::storage::Storage;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::Extension;
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// WebSocket query parameters
#[derive(Deserialize)]
pub struct WsQuery {
    token: String,
    #[serde(default)]
    session_id: Option<String>,
}

/// WebSocket connection handler
pub async fn websocket_handler<S: Storage + 'static>(
    ws: WebSocketUpgrade,
    State(router): State<Arc<Router<S>>>,
    Extension(auth): Extension<AuthManager<S>>,
    Query(params): Query<WsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // Validate token
    let user_id = auth.validate_token_str(&params.token).await?;

    debug!(
        "WebSocket connection: user={}, session={:?}",
        user_id, params.session_id
    );

    // Accept WebSocket connection
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, router, user_id, params.session_id)))
}

/// Handle an individual WebSocket connection
async fn handle_socket<S: Storage + 'static>(
    socket: WebSocket,
    router: Arc<Router<S>>,
    user_id: String,
    _session_id: Option<String>,
) {
    let (mut sender, mut receiver) = socket.split();

    info!("WebSocket connected: user={}", user_id);

    // Get or create session
    let session = match router.get_or_create_session_api(&user_id, "web").await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to get session: {}", e);
            let _ = sender
                .send(Message::Text(
                    serde_json::to_string(&WebSocketMessage::Error {
                        error: "Failed to create session".to_string(),
                        error_code: 500,
                    })
                    .unwrap_or_default(),
                ))
                .await;
            return;
        }
    };

    // Send connected message
    let connected = WebSocketMessage::Connected {
        session_id: session.id.clone(),
    };
    if let Ok(json) = connected.to_json() {
        let _ = sender.send(Message::Text(json)).await;
    }

    // Spawn keepalive task with channel
    let (tx, mut rx) = mpsc::channel(10);
    let keepalive_handle = tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let ping = WebSocketMessage::Ping;
            if let Ok(json) = ping.to_json() {
                if tx.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Main message loop
    let session_id = session.id.clone();
    let user_id_clone = user_id.clone();
    let router_clone = router.clone();

    loop {
        tokio::select! {
            msg = receiver.next() => {
                let msg = match msg {
                    Some(Ok(m)) => m,
                    _ => break,
                };
                match msg {
                    Message::Text(text) => {
                        // Parse incoming message
                        match serde_json::from_str::<WebSocketMessage>(&text) {
                            Ok(WebSocketMessage::Message { content }) => {
                                debug!("Message from {}: {}", user_id_clone, content);

                                // Process message and stream response
                                if let Err(e) = process_and_stream(
                                    &mut sender,
                                    router_clone.clone(),
                                    user_id_clone.clone(),
                                    session_id.clone(),
                                    content,
                                )
                                .await
                                {
                                    error!("Error processing message: {:?}", e);
                                    let err_msg = WebSocketMessage::Error {
                                        error: "Failed to process message".to_string(),
                                        error_code: 500,
                                    };
                                    if let Ok(json) = err_msg.to_json() {
                                        let _ = sender.send(Message::Text(json)).await;
                                    }
                                }
                            }
                            Ok(WebSocketMessage::ToolApprovalResponse {
                                request_id,
                                approved,
                                use_sandbox,
                                remember_for_session,
                            }) => {
                                debug!(
                                    "Tool approval response from {}: request_id={}, approved={}, use_sandbox={}, remember={}",
                                    user_id_clone, request_id, approved, use_sandbox, remember_for_session
                                );

                                // Route to ApprovalManager
                                if let Ok(approval_mgr) = router_clone.get_approval_manager() {
                                    approval_mgr
                                        .submit_approval_response(
                                            &request_id,
                                            approved,
                                            use_sandbox,
                                            remember_for_session,
                                        )
                                        .await;

                                    debug!(
                                        "Tool approval response stored: request_id={}, approved={}",
                                        request_id, approved
                                    );
                                } else {
                                    warn!(
                                        "Failed to access approval manager for response: request_id={}",
                                        request_id
                                    );
                                    let err_msg = WebSocketMessage::Error {
                                        error: "Approval manager not available".to_string(),
                                        error_code: 500,
                                    };
                                    if let Ok(json) = err_msg.to_json() {
                                        let _ = sender.send(Message::Text(json)).await;
                                    }
                                }
                            }
                            Ok(WebSocketMessage::Pong) => {
                                debug!("Received pong from {}", user_id_clone);
                            }
                            Ok(_) => {
                                warn!("Unexpected message type from client");
                            }
                            Err(e) => {
                                warn!("Failed to parse message: {}", e);
                                let err_msg = WebSocketMessage::Error {
                                    error: "Invalid message format".to_string(),
                                    error_code: 400,
                                };
                                if let Ok(json) = err_msg.to_json() {
                                    let _ = sender.send(Message::Text(json)).await;
                                }
                            }
                        }
                    }
                    Message::Close(_) => {
                        info!("WebSocket closed by client: user={}", user_id_clone);
                        break;
                    }
                    Message::Ping(data) => {
                        debug!("Received ping from client");
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    _ => {
                        debug!("Received other message type");
                    }
                }
            }
            msg = rx.recv() => {
                // Keepalive message from channel
                if let Some(m) = msg {
                    let _ = sender.send(m).await;
                } else {
                    break;
                }
            }
        }
    }

    // Cleanup
    keepalive_handle.abort();
    info!("WebSocket disconnected: user={}", user_id);
}

/// Process message and stream response
async fn process_and_stream<S: Storage + 'static>(
    sender: &mut SplitSink<WebSocket, Message>,
    router: Arc<Router<S>>,
    user_id: String,
    _session_id: String,
    content: String,
) -> Result<(), ApiError> {
    // Validate input
    if content.is_empty() {
        return Err(ApiError::BadRequest("message cannot be empty".to_string()));
    }

    if content.len() > 10000 {
        return Err(ApiError::BadRequest(
            "message too long (max 10000 chars)".to_string(),
        ));
    }

    let message_id = format!("msg-{}", uuid::Uuid::new_v4());
    let start = std::time::Instant::now();

    // Send start notification
    let start_msg = WebSocketMessage::Start {
        session_id: message_id.clone(),
        message_id: message_id.clone(),
    };
    if let Ok(json) = start_msg.to_json() {
        sender
            .send(Message::Text(json))
            .await
            .map_err(|_| ApiError::InternalError("Failed to send message".to_string()))?;
    }

    // Get streaming receiver from router
    let mut receiver = router
        .handle_message_stream(&user_id, "web", &content)
        .await
        .map_err(|e| {
            error!("Failed to handle message: {}", e);
            ApiError::InternalError("Failed to process message".to_string())
        })?;

    // Consume stream events
    let mut total_tokens = 0;
    let final_model: String;
    while let Some(event) = receiver.recv().await {
        match event {
            StreamEvent::Delta(text) => {
                // Send content chunk
                let stream_msg = WebSocketMessage::Stream { content: text };
                if let Ok(json) = stream_msg.to_json() {
                    if sender.send(Message::Text(json)).await.is_err() {
                        // Client disconnected
                        return Ok(());
                    }
                }
            }
            StreamEvent::ToolStart {
                name,
                attempt,
                max_attempts,
            } => {
                // Send tool start event
                let tool_msg = WebSocketMessage::ToolUse {
                    name,
                    status: "running".to_string(),
                    output: None,
                    error: None,
                    execution_time_ms: None,
                    attempt,
                    max_attempts,
                };
                if let Ok(json) = tool_msg.to_json() {
                    if sender.send(Message::Text(json)).await.is_err() {
                        return Ok(());
                    }
                }
            }
            StreamEvent::ToolEnd {
                name,
                result,
                execution_time_ms,
                attempt,
            } => {
                // Send tool end event
                let tool_msg = WebSocketMessage::ToolUse {
                    name,
                    status: "done".to_string(),
                    output: Some(result),
                    error: None,
                    execution_time_ms,
                    attempt,
                    max_attempts: None,
                };
                if let Ok(json) = tool_msg.to_json() {
                    if sender.send(Message::Text(json)).await.is_err() {
                        return Ok(());
                    }
                }
            }
            StreamEvent::Done { model, usage } => {
                // Extract final stats
                final_model = model;
                if let Some(u) = usage {
                    total_tokens = u.total_tokens;
                }

                // Send end notification
                let latency_ms = start.elapsed().as_millis() as u64;
                let end_msg = WebSocketMessage::End {
                    message_id,
                    total_tokens,
                    model: final_model,
                    latency_ms,
                };
                if let Ok(json) = end_msg.to_json() {
                    let _ = sender.send(Message::Text(json)).await;
                }
                break;
            }
            StreamEvent::ApprovalRequested {
                request_id,
                tool_name,
                arguments,
                policy,
                sandbox_available,
            } => {
                debug!(
                    "Forwarding approval request: request_id={}, tool={}",
                    request_id, tool_name
                );
                let approval_msg = WebSocketMessage::ToolApprovalRequest {
                    request_id,
                    tool: tool_name,
                    arguments,
                    policy,
                    sandbox_available,
                };
                if let Ok(json) = approval_msg.to_json() {
                    let _ = sender.send(Message::Text(json)).await;
                }
            }
            StreamEvent::Error(msg) => {
                error!("Stream error: {}", msg);
                let err_msg = WebSocketMessage::Error {
                    error: msg,
                    error_code: 500,
                };
                if let Ok(json) = err_msg.to_json() {
                    let _ = sender.send(Message::Text(json)).await;
                }
                break;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_query_parsing() {
        let query_str = "token=test-token&session_id=sess-123";
        let query: WsQuery = serde_urlencoded::from_str(query_str).unwrap();
        assert_eq!(query.token, "test-token");
        assert_eq!(query.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn test_ws_query_without_session() {
        let query_str = "token=test-token";
        let query: WsQuery = serde_urlencoded::from_str(query_str).unwrap();
        assert_eq!(query.token, "test-token");
        assert_eq!(query.session_id, None);
    }
}
