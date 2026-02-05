use super::server::McpServer;
use super::types::{JsonRpcNotification, JsonRpcRequest};
use crate::core::events::{subscribe, SystemEvent};
use axum::{
    extract::Query,
    response::{
        sse::{Event, Sse},
        IntoResponse,
    },
    Json,
};
use futures::stream::Stream;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::{collections::HashMap, convert::Infallible, sync::Arc};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};
use uuid::Uuid;

// Global session manager
static SESSION_MANAGER: Lazy<McpSessionManager> = Lazy::new(McpSessionManager::new);

type SseSender = mpsc::UnboundedSender<Result<Event, Infallible>>;

pub struct McpSessionManager {
    sessions: Arc<RwLock<HashMap<String, SseSender>>>,
    server: Arc<McpServer>,
}

impl McpSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            server: Arc::new(McpServer::new()),
        }
    }
}

impl Default for McpSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl McpSessionManager {
    pub async fn create_session(&self) -> (String, impl Stream<Item = Result<Event, Infallible>>) {
        let session_id = Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel();

        // Register session
        self.sessions
            .write()
            .await
            .insert(session_id.clone(), tx.clone());

        // Spawn system event listener for this session
        let session_id_clone = session_id.clone();
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let mut event_rx = subscribe();
            while let Ok(event) = event_rx.recv().await {
                match event {
                    SystemEvent::ToolUpdated(_) | SystemEvent::ToolRemoved(_) => {
                        // Notify client that tools changed
                        let notification = JsonRpcNotification {
                            jsonrpc: "2.0".to_string(),
                            method: "notifications/tools/list_changed".to_string(),
                            params: None,
                        };

                        if let Ok(json) = serde_json::to_string(&notification) {
                            if tx_clone
                                .send(Ok(Event::default().event("message").data(json)))
                                .is_err()
                            {
                                break; // Channel closed
                            }
                        }
                    }
                    _ => {}
                }
            }
            debug!("Event listener stopped for session {}", session_id_clone);
        });

        (
            session_id,
            tokio_stream::wrappers::UnboundedReceiverStream::new(rx),
        )
    }

    pub async fn get_sender(&self, session_id: &str) -> Option<SseSender> {
        self.sessions.read().await.get(session_id).cloned()
    }

    pub async fn handle_message(&self, session_id: &str, req: JsonRpcRequest) {
        if let Some(tx) = self.get_sender(session_id).await {
            let server = self.server.clone();
            tokio::spawn(async move {
                // Ensure elevated permissions for MCP session
                if let Some(policy) = crate::get_tool_policy_engine() {
                    policy.set_elevated("mcp-session", true).await;
                }

                let response = server.handle_request(req).await;
                if let Ok(json) = serde_json::to_string(&response) {
                    let _ = tx.send(Ok(Event::default().event("message").data(json)));
                }
            });
        } else {
            error!("Session not found: {}", session_id);
        }
    }
}

// Handlers

#[derive(Deserialize)]
pub struct SseQuery {}

pub async fn sse_handler(Query(_params): Query<SseQuery>) -> impl IntoResponse {
    let (session_id, stream) = SESSION_MANAGER.create_session().await;

    info!("New MCP session started: {}", session_id);

    // Send the endpoint event as the first event
    let endpoint_url = format!("/mcp/messages?sessionId={}", session_id);
    let endpoint_event = Event::default().event("endpoint").data(endpoint_url);

    if let Some(tx) = SESSION_MANAGER.get_sender(&session_id).await {
        let _ = tx.send(Ok(endpoint_event));
    }

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

#[derive(Deserialize)]
pub struct MessageQuery {
    #[serde(rename = "sessionId")]
    session_id: String,
}

pub async fn messages_handler(
    Query(params): Query<MessageQuery>,
    Json(req): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    debug!("Received MCP message for session {}", params.session_id);
    SESSION_MANAGER
        .handle_message(&params.session_id, req)
        .await;
    axum::http::StatusCode::ACCEPTED
}
