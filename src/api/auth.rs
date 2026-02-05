use crate::api::error::ApiError;
use crate::storage::Storage;
use axum::{extract::Request, http::HeaderMap, middleware::Next, response::Response};
use std::sync::Arc;

/// API authentication manager
#[derive(Clone)]
pub struct AuthManager<S: Storage> {
    /// Valid API tokens (from config)
    valid_tokens: Arc<Vec<String>>,
    /// Storage for database token lookup
    storage: S,
}

impl<S: Storage + 'static> AuthManager<S> {
    pub fn new(tokens: Vec<String>, storage: S) -> Self {
        Self {
            valid_tokens: Arc::new(tokens),
            storage,
        }
    }

    /// Validate bearer token from headers
    pub async fn validate_token(&self, headers: &HeaderMap) -> Result<String, ApiError> {
        let auth_header = headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| ApiError::Unauthorized("Missing authorization header".to_string()))?;

        // Check for Bearer token format
        if !auth_header.starts_with("Bearer ") {
            return Err(ApiError::Unauthorized(
                "Invalid authorization header format. Use: Bearer <token>".to_string(),
            ));
        }

        let token = &auth_header[7..]; // Skip "Bearer "

        if token.is_empty() {
            return Err(ApiError::Unauthorized("Token cannot be empty".to_string()));
        }

        // First check config tokens (fast path)
        if self.valid_tokens.contains(&token.to_string()) {
            return Ok(token.to_string());
        }

        // Then check database tokens
        if self.validate_db_token(token).await {
            return Ok(token.to_string());
        }

        tracing::warn!("Invalid token attempt");
        Err(ApiError::Unauthorized("Invalid token".to_string()))
    }

    /// Validate a raw token string (for WebSocket query-param auth)
    pub async fn validate_token_str(&self, token: &str) -> Result<String, ApiError> {
        if token.is_empty() {
            return Err(ApiError::Unauthorized("Token cannot be empty".to_string()));
        }

        // First check config tokens
        if self.valid_tokens.contains(&token.to_string()) {
            return Ok(Self::token_to_user_id(token));
        }

        // Then check database tokens
        if let Some(identity) = self.get_db_identity(token).await {
            return Ok(identity.user_id);
        }

        tracing::warn!("Invalid token attempt (ws)");
        Err(ApiError::Unauthorized("Invalid token".to_string()))
    }

    /// Check if token exists in database
    async fn validate_db_token(&self, token: &str) -> bool {
        self.get_db_identity(token).await.is_some()
    }

    /// Get identity from database for token
    async fn get_db_identity(&self, token: &str) -> Option<crate::storage::Identity> {
        self.storage
            .get_identity("api_token", token)
            .await
            .ok()
            .flatten()
    }

    /// Extract user ID from token
    /// Token format: "web-user-<name>" → user ID is "<name>"
    /// Or just use token as user ID
    pub fn token_to_user_id(token: &str) -> String {
        if let Some(user_part) = token.strip_prefix("web-user-") {
            user_part.to_string()
        } else {
            token.to_string()
        }
    }

    /// Middleware for protecting routes
    pub async fn auth_middleware(
        axum::extract::State(auth_manager): axum::extract::State<AuthManager<S>>,
        headers: HeaderMap,
        mut request: Request,
        next: Next,
    ) -> Result<Response, ApiError> {
        // Validate token
        let token = auth_manager.validate_token(&headers).await?;

        // For database tokens, get user_id from identity
        let user_id = if let Some(identity) = auth_manager.get_db_identity(&token).await {
            identity.user_id
        } else {
            Self::token_to_user_id(&token)
        };

        // Store user ID in request extensions for use in handlers
        request.extensions_mut().insert(user_id);
        request.extensions_mut().insert(token);
        request.extensions_mut().insert(auth_manager);

        Ok(next.run(request).await)
    }
}

#[cfg(test)]
mod tests {
    // Tests require mock storage, skipped for now
}
