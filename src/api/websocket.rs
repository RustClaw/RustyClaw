use crate::api::{ApiError, WebSocketMessage};
use crate::core::Router;
use crate::storage::Storage;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
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
    Query(params): Query<WsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // Validate token (same as auth middleware)
    let user_id = crate::api::AuthManager::token_to_user_id(&params.token);

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
    session_id: String,
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
        session_id: session_id.clone(),
        message_id: message_id.clone(),
    };
    if let Ok(json) = start_msg.to_json() {
        sender
            .send(Message::Text(json))
            .await
            .map_err(|_| ApiError::InternalError("Failed to send message".to_string()))?;
    }

    // Process message through router
    let response = router
        .handle_message(&user_id, "web", &content)
        .await
        .map_err(|e| {
            error!("Failed to handle message: {}", e);
            ApiError::InternalError("Failed to process message".to_string())
        })?;

    // Stream response in chunks (for now, send whole response)
    // In production, could chunk larger responses
    let stream_msg = WebSocketMessage::Stream {
        content: response.content.clone(),
    };
    if let Ok(json) = stream_msg.to_json() {
        sender
            .send(Message::Text(json))
            .await
            .map_err(|_| ApiError::InternalError("Failed to send message".to_string()))?;
    }

    // Send end notification
    let latency_ms = start.elapsed().as_millis() as u64;
    let end_msg = WebSocketMessage::End {
        message_id,
        total_tokens: response.tokens.unwrap_or(0),
        model: response.model,
        latency_ms,
    };
    if let Ok(json) = end_msg.to_json() {
        sender
            .send(Message::Text(json))
            .await
            .map_err(|_| ApiError::InternalError("Failed to send message".to_string()))?;
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
