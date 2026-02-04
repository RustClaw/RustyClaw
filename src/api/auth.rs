use crate::api::error::ApiError;
use axum::{extract::Request, http::HeaderMap, middleware::Next, response::Response};
use std::sync::Arc;

/// API authentication manager
#[derive(Debug, Clone)]
pub struct AuthManager {
    /// Valid API tokens (from config)
    valid_tokens: Arc<Vec<String>>,
}

impl AuthManager {
    pub fn new(tokens: Vec<String>) -> Self {
        Self {
            valid_tokens: Arc::new(tokens),
        }
    }

    /// Validate bearer token from headers
    pub fn validate_token(&self, headers: &HeaderMap) -> Result<String, ApiError> {
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

        // Validate token is in allowed list
        if !self.valid_tokens.contains(&token.to_string()) {
            tracing::warn!("Invalid token attempt");
            return Err(ApiError::Unauthorized("Invalid token".to_string()));
        }

        Ok(token.to_string())
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

    /// Validate a raw token string (for WebSocket query-param auth)
    pub fn validate_token_str(&self, token: &str) -> Result<String, ApiError> {
        if token.is_empty() {
            return Err(ApiError::Unauthorized("Token cannot be empty".to_string()));
        }
        if !self.valid_tokens.contains(&token.to_string()) {
            tracing::warn!("Invalid token attempt (ws)");
            return Err(ApiError::Unauthorized("Invalid token".to_string()));
        }
        Ok(Self::token_to_user_id(token))
    }

    /// Middleware for protecting routes
    pub async fn auth_middleware(
        axum::extract::State(auth_manager): axum::extract::State<AuthManager>,
        headers: HeaderMap,
        mut request: Request,
        next: Next,
    ) -> Result<Response, ApiError> {
        // Validate token
        let token = auth_manager.validate_token(&headers)?;
        let user_id = Self::token_to_user_id(&token);

        // Store user ID in request extensions for use in handlers
        request.extensions_mut().insert(user_id);
        request.extensions_mut().insert(token);
        request.extensions_mut().insert(auth_manager);

        Ok(next.run(request).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_validation() {
        let tokens = vec!["test-token".to_string(), "another-token".to_string()];
        let auth = AuthManager::new(tokens);

        // Valid token
        assert!(auth
            .validate_token(&{
                let mut headers = HeaderMap::new();
                headers.insert("authorization", "Bearer test-token".parse().unwrap());
                headers
            })
            .is_ok());

        // Invalid token
        assert!(auth
            .validate_token(&{
                let mut headers = HeaderMap::new();
                headers.insert("authorization", "Bearer invalid-token".parse().unwrap());
                headers
            })
            .is_err());

        // Missing header
        assert!(auth.validate_token(&HeaderMap::new()).is_err());
    }

    #[test]
    fn test_invalid_auth_format() {
        let auth = AuthManager::new(vec!["test-token".to_string()]);

        // Wrong format
        let result = auth.validate_token(&{
            let mut headers = HeaderMap::new();
            headers.insert("authorization", "Basic dGVzdDp0ZXN0".parse().unwrap());
            headers
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_token_to_user_id() {
        assert_eq!(AuthManager::token_to_user_id("web-user-alice"), "alice");
        assert_eq!(AuthManager::token_to_user_id("web-user-bob"), "bob");
        assert_eq!(
            AuthManager::token_to_user_id("custom-token"),
            "custom-token"
        );
    }

    #[test]
    fn test_empty_token() {
        let auth = AuthManager::new(vec!["test-token".to_string()]);

        let result = auth.validate_token(&{
            let mut headers = HeaderMap::new();
            headers.insert("authorization", "Bearer ".parse().unwrap());
            headers
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_token_str() {
        let tokens = vec!["test-token".to_string(), "another-token".to_string()];
        let auth = AuthManager::new(tokens);

        // Valid token
        assert!(auth.validate_token_str("test-token").is_ok());
        assert!(auth.validate_token_str("another-token").is_ok());

        // Invalid token
        assert!(auth.validate_token_str("invalid-token").is_err());

        // Empty token
        assert!(auth.validate_token_str("").is_err());
    }
}
