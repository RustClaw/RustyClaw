pub mod auth;
pub mod error;
pub mod response;
pub mod routes;
pub mod websocket;

use crate::core::Router;
use crate::storage::Storage;
use anyhow::{Context, Result};
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router as AxumRouter};
use std::sync::Arc;
use tracing::info;

pub use auth::AuthManager;
pub use error::ApiError;
pub use response::*;

/// Web API adapter for HTTP/WebSocket access
pub struct WebApiAdapter<S: Storage> {
    router: Arc<Router<S>>,
    auth_manager: AuthManager,
    host: String,
    port: u16,
    api_path: String,
    ws_path: String,
}

impl<S: Storage + 'static> WebApiAdapter<S> {
    /// Create new Web API adapter
    pub fn new(router: Arc<Router<S>>, host: String, port: u16, tokens: Vec<String>) -> Self {
        let auth_manager = AuthManager::new(tokens);

        Self {
            router,
            auth_manager,
            host,
            port,
            api_path: "/api".to_string(),
            ws_path: "/ws".to_string(),
        }
    }

    /// Set custom API path
    pub fn with_api_path(mut self, path: String) -> Self {
        self.api_path = path;
        self
    }

    /// Set custom WebSocket path
    pub fn with_ws_path(mut self, path: String) -> Self {
        self.ws_path = path;
        self
    }

    /// Build Axum router with all endpoints
    fn build_routes(&self) -> AxumRouter {
        // Public endpoints (no auth required)
        let public_routes = AxumRouter::new()
            .route("/health", get(health_handler))
            .route(
                &self.ws_path,
                axum::routing::get(websocket::websocket_handler),
            )
            .with_state(self.router.clone());

        // Protected endpoints (auth required)
        let api_routes = AxumRouter::new()
            // Session endpoints
            .route(
                &format!("{}/sessions", self.api_path),
                post(routes::create_session),
            )
            .route(
                &format!("{}/sessions", self.api_path),
                get(routes::list_sessions),
            )
            .route(
                &format!("{}/sessions/:id", self.api_path),
                get(routes::get_session),
            )
            .route(
                &format!("{}/sessions/:id", self.api_path),
                delete(routes::delete_session),
            )
            // Chat endpoint
            .route(&format!("{}/chat", self.api_path), post(routes::chat))
            // Message endpoints
            .route(
                &format!("{}/messages", self.api_path),
                get(routes::list_messages),
            )
            .route(
                &format!("{}/messages/:id", self.api_path),
                get(routes::get_message),
            )
            // Models endpoints
            .route(
                &format!("{}/models", self.api_path),
                get(routes::list_models),
            )
            .route(
                &format!("{}/models/:name/load", self.api_path),
                post(routes::load_model),
            )
            .with_state(self.router.clone())
            .layer(axum::middleware::from_fn_with_state(
                self.auth_manager.clone(),
                AuthManager::auth_middleware,
            ));

        // Combine routes
        AxumRouter::new()
            .merge(public_routes)
            .merge(api_routes)
            .layer(DefaultBodyLimit::max(1024 * 1024 * 10)) // 10MB
            .layer(axum::middleware::from_fn(logging_middleware))
    }

    /// Start the Web API server
    pub async fn start(&self) -> Result<()> {
        let app = self.build_routes();
        let addr = format!("{}:{}", self.host, self.port);

        info!(
            "Starting Web API on {} (Health: /health, API: {}, WebSocket: {})",
            addr, self.api_path, self.ws_path
        );

        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .context("Failed to bind server")?;

        axum::serve(listener, app).await.context("Server error")?;

        Ok(())
    }
}

/// Health check handler
async fn health_handler() -> (StatusCode, Json<HealthResponse>) {
    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            gateway: "rustyclaw".to_string(),
        }),
    )
}

/// Logging middleware
async fn logging_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let method = request.method().clone();
    let uri = request.uri().clone();

    let response = next.run(request).await;

    let status = response.status();
    tracing::info!("{} {} → {}", method, uri, status);

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_api_creation() {
        // Note: Can't fully test without actual Storage implementation
        // This just verifies the struct can be created
        let tokens = vec!["test-token".to_string()];

        // We'd need a real Router here in integration tests
        // For now, just verify the AuthManager works
        let auth = AuthManager::new(tokens);
        assert!(auth
            .validate_token(&{
                let mut headers = axum::http::HeaderMap::new();
                headers.insert("authorization", "Bearer test-token".parse().unwrap());
                headers
            })
            .is_ok());
    }

    #[test]
    fn test_health_response_format() {
        let response = HealthResponse {
            status: "ok".to_string(),
            version: "0.1.0".to_string(),
            gateway: "rustyclaw".to_string(),
        };

        assert_eq!(response.status, "ok");
        assert_eq!(response.gateway, "rustyclaw");
    }
}
