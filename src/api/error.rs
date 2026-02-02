use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// API error types
#[derive(Debug)]
pub enum ApiError {
    /// Bad request (400)
    BadRequest(String),

    /// Unauthorized (401)
    Unauthorized(String),

    /// Forbidden (403)
    Forbidden(String),

    /// Not found (404)
    NotFound(String),

    /// Conflict (409)
    Conflict(String),

    /// Rate limit exceeded (429)
    RateLimited { retry_after: u64 },

    /// Internal server error (500)
    InternalError(String),

    /// Service unavailable (503)
    ServiceUnavailable(String),
}

impl ApiError {
    /// Get HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    /// Get error code for JSON response
    pub fn error_code(&self) -> u32 {
        match self {
            Self::BadRequest(_) => 400,
            Self::Unauthorized(_) => 401,
            Self::Forbidden(_) => 403,
            Self::NotFound(_) => 404,
            Self::Conflict(_) => 409,
            Self::RateLimited { .. } => 429,
            Self::InternalError(_) => 500,
            Self::ServiceUnavailable(_) => 503,
        }
    }

    /// Get error message
    pub fn message(&self) -> String {
        match self {
            Self::BadRequest(msg) => msg.clone(),
            Self::Unauthorized(msg) => msg.clone(),
            Self::Forbidden(msg) => msg.clone(),
            Self::NotFound(msg) => msg.clone(),
            Self::Conflict(msg) => msg.clone(),
            Self::RateLimited { .. } => "Rate limit exceeded".to_string(),
            Self::InternalError(msg) => msg.clone(),
            Self::ServiceUnavailable(msg) => msg.clone(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let error_code = self.error_code();
        let message = self.message();

        let mut body = json!({
            "error": message,
            "error_code": error_code,
        });

        // Add retry_after for rate limiting
        if let Self::RateLimited { retry_after } = self {
            body["retry_after"] = json!(retry_after);
        }

        (status, axum::Json(body)).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!("Internal error: {:?}", err);
        Self::InternalError("Internal server error".to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        tracing::warn!("JSON error: {}", err);
        Self::BadRequest("Invalid JSON".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(
            ApiError::BadRequest("test".into()).status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ApiError::Unauthorized("test".into()).status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            ApiError::NotFound("test".into()).status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ApiError::RateLimited { retry_after: 60 }.status_code(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(ApiError::BadRequest("test".into()).error_code(), 400);
        assert_eq!(ApiError::Unauthorized("test".into()).error_code(), 401);
        assert_eq!(ApiError::InternalError("test".into()).error_code(), 500);
    }

    #[test]
    fn test_error_message() {
        let msg = "test message";
        assert_eq!(ApiError::BadRequest(msg.into()).message(), msg);
    }
}
