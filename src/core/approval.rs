use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Represents a pending tool approval request
#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub request_id: String,
    pub tool_name: String,
    pub arguments: String,
    pub policy: String,
    pub sandbox_available: bool,
    pub timestamp: Instant,
}

/// User response to an approval request
#[derive(Debug, Clone)]
pub struct ApprovalResponse {
    pub approved: bool,
    pub use_sandbox: bool,
    pub remember_for_session: bool,
    pub timestamp: Instant,
}

/// Manages tool approval requests and responses
///
/// This manager handles the asynchronous approval flow:
/// 1. Server creates an approval request with a unique ID
/// 2. Request is sent to client via WebSocket
/// 3. Server waits for response from client
/// 4. Client responds with approval decision
/// 5. Server continues tool execution based on response
#[derive(Clone)]
pub struct ApprovalManager {
    /// Map of session_id → (request_id → PendingApproval)
    pending: Arc<RwLock<HashMap<String, HashMap<String, PendingApproval>>>>,
    /// Map of request_id → ApprovalResponse
    responses: Arc<RwLock<HashMap<String, ApprovalResponse>>>,
}

impl ApprovalManager {
    /// Create a new ApprovalManager
    pub fn new() -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            responses: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new approval request and return the request_id
    pub async fn create_approval_request(
        &self,
        session_id: &str,
        tool_name: &str,
        arguments: &str,
        policy: &str,
        sandbox_available: bool,
    ) -> String {
        let request_id = Uuid::new_v4().to_string();
        let approval = PendingApproval {
            request_id: request_id.clone(),
            tool_name: tool_name.to_string(),
            arguments: arguments.to_string(),
            policy: policy.to_string(),
            sandbox_available,
            timestamp: Instant::now(),
        };

        // Store pending approval
        let mut pending = self.pending.write().await;
        pending
            .entry(session_id.to_string())
            .or_insert_with(HashMap::new)
            .insert(request_id.clone(), approval);

        tracing::debug!(
            "Created approval request: request_id={}, tool={}, session={}",
            request_id,
            tool_name,
            session_id
        );

        request_id
    }

    /// Wait for approval response with timeout
    ///
    /// Returns Some(response) if approved/denied by user
    /// Returns None if timeout expires (auto-deny)
    pub async fn wait_for_approval(
        &self,
        request_id: &str,
        timeout_secs: u64,
    ) -> Option<ApprovalResponse> {
        let start = Instant::now();
        let timeout_duration = std::time::Duration::from_secs(timeout_secs);

        loop {
            {
                let responses = self.responses.read().await;
                if let Some(response) = responses.get(request_id) {
                    tracing::debug!(
                        "Got approval response: request_id={}, approved={}",
                        request_id,
                        response.approved
                    );
                    return Some(response.clone());
                }
            }

            if start.elapsed() > timeout_duration {
                tracing::warn!(
                    "Approval request timed out after {}s: request_id={}",
                    timeout_secs,
                    request_id
                );
                return None; // Timeout = deny
            }

            // Sleep briefly before checking again
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    /// Submit an approval response from the client
    pub async fn submit_approval_response(
        &self,
        request_id: &str,
        approved: bool,
        use_sandbox: bool,
        remember_for_session: bool,
    ) {
        let response = ApprovalResponse {
            approved,
            use_sandbox,
            remember_for_session,
            timestamp: Instant::now(),
        };

        let mut responses = self.responses.write().await;
        responses.insert(request_id.to_string(), response);

        tracing::debug!(
            "Stored approval response: request_id={}, approved={}",
            request_id,
            approved
        );
    }

    /// Get a pending approval request
    pub async fn get_pending_approval(
        &self,
        session_id: &str,
        request_id: &str,
    ) -> Option<PendingApproval> {
        let pending = self.pending.read().await;
        pending
            .get(session_id)
            .and_then(|session_requests| session_requests.get(request_id))
            .cloned()
    }

    /// Clear all approvals for a session
    pub async fn clear_session_approvals(&self, session_id: &str) {
        let mut pending = self.pending.write().await;
        pending.remove(session_id);

        tracing::debug!("Cleared all approvals for session: {}", session_id);
    }

    /// Clear all approval responses (useful for cleanup)
    pub async fn clear_all_responses(&self) {
        let mut responses = self.responses.write().await;
        responses.clear();

        tracing::debug!("Cleared all approval responses");
    }

    /// Get statistics about pending approvals
    pub async fn get_stats(&self) -> ApprovalStats {
        let pending = self.pending.read().await;
        let responses = self.responses.read().await;

        let total_pending: usize = pending.values().map(|m| m.len()).sum();

        ApprovalStats {
            total_pending,
            total_responses: responses.len(),
            sessions_with_pending: pending.len(),
        }
    }
}

impl Default for ApprovalManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about approval manager state
#[derive(Debug, Clone)]
pub struct ApprovalStats {
    pub total_pending: usize,
    pub total_responses: usize,
    pub sessions_with_pending: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_approval_request() {
        let manager = ApprovalManager::new();
        let request_id = manager
            .create_approval_request("session-1", "bash", r#"{"cmd":"ls"}"#, "elevated", true)
            .await;

        assert!(!request_id.is_empty());
        assert_eq!(request_id.len(), 36); // UUID format: 8-4-4-4-12 = 36 chars
    }

    #[tokio::test]
    async fn test_submit_and_wait_for_approval() {
        let manager = ApprovalManager::new();
        let request_id = manager
            .create_approval_request("session-1", "bash", r#"{"cmd":"ls"}"#, "elevated", true)
            .await;

        // Submit response
        manager
            .submit_approval_response(&request_id, true, true, false)
            .await;

        // Wait for response
        let response = manager.wait_for_approval(&request_id, 5).await;
        assert!(response.is_some());

        let resp = response.unwrap();
        assert!(resp.approved);
        assert!(resp.use_sandbox);
        assert!(!resp.remember_for_session);
    }

    #[tokio::test]
    async fn test_approval_timeout() {
        let manager = ApprovalManager::new();
        let request_id = manager
            .create_approval_request("session-1", "bash", r#"{"cmd":"ls"}"#, "elevated", true)
            .await;

        // Wait for response with 1 second timeout, don't submit response
        let response = manager.wait_for_approval(&request_id, 1).await;
        assert!(response.is_none()); // Timeout = None
    }

    #[tokio::test]
    async fn test_get_pending_approval() {
        let manager = ApprovalManager::new();
        let request_id = manager
            .create_approval_request("session-1", "bash", r#"{"cmd":"ls"}"#, "elevated", true)
            .await;

        let pending = manager.get_pending_approval("session-1", &request_id).await;
        assert!(pending.is_some());

        let approval = pending.unwrap();
        assert_eq!(approval.tool_name, "bash");
        assert_eq!(approval.policy, "elevated");
        assert!(approval.sandbox_available);
    }

    #[tokio::test]
    async fn test_clear_session_approvals() {
        let manager = ApprovalManager::new();
        let request_id = manager
            .create_approval_request("session-1", "bash", r#"{"cmd":"ls"}"#, "elevated", true)
            .await;

        // Clear session
        manager.clear_session_approvals("session-1").await;

        // Request should be gone
        let pending = manager.get_pending_approval("session-1", &request_id).await;
        assert!(pending.is_none());
    }

    #[tokio::test]
    async fn test_approval_denial() {
        let manager = ApprovalManager::new();
        let request_id = manager
            .create_approval_request("session-1", "bash", r#"{"cmd":"ls"}"#, "elevated", true)
            .await;

        // Submit denial
        manager
            .submit_approval_response(&request_id, false, false, false)
            .await;

        // Wait for response
        let response = manager.wait_for_approval(&request_id, 5).await;
        assert!(response.is_some());

        let resp = response.unwrap();
        assert!(!resp.approved);
    }

    #[tokio::test]
    async fn test_multiple_sessions() {
        let manager = ApprovalManager::new();

        let req1 = manager
            .create_approval_request("session-1", "bash", "{}", "elevated", true)
            .await;
        let req2 = manager
            .create_approval_request("session-2", "bash", "{}", "elevated", true)
            .await;

        // Verify both are pending in separate sessions
        let pending1 = manager.get_pending_approval("session-1", &req1).await;
        let pending2 = manager.get_pending_approval("session-2", &req2).await;

        assert!(pending1.is_some());
        assert!(pending2.is_some());

        // Clear one session
        manager.clear_session_approvals("session-1").await;

        let pending1_after = manager.get_pending_approval("session-1", &req1).await;
        let pending2_after = manager.get_pending_approval("session-2", &req2).await;

        assert!(pending1_after.is_none());
        assert!(pending2_after.is_some());
    }

    #[tokio::test]
    async fn test_get_stats() {
        let manager = ApprovalManager::new();

        let req1 = manager
            .create_approval_request("session-1", "bash", "{}", "elevated", true)
            .await;
        let _req2 = manager
            .create_approval_request("session-1", "python", "{}", "elevated", true)
            .await;
        let _req3 = manager
            .create_approval_request("session-2", "bash", "{}", "elevated", true)
            .await;

        // Submit one response
        manager
            .submit_approval_response(&req1, true, true, false)
            .await;

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_pending, 3);
        assert_eq!(stats.total_responses, 1);
        assert_eq!(stats.sessions_with_pending, 2);
    }
}
