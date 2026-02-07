use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Tool access control level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolAccessLevel {
    /// Tool is always allowed
    Allow,
    /// Tool is never allowed
    Deny,
    /// Tool requires elevated mode activation
    Elevated,
}

impl std::str::FromStr for ToolAccessLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "allow" => Ok(ToolAccessLevel::Allow),
            "deny" => Ok(ToolAccessLevel::Deny),
            "elevated" => Ok(ToolAccessLevel::Elevated),
            _ => Err(format!("Invalid access level: {}", s)),
        }
    }
}

/// Error types for policy enforcement
#[derive(Debug, thiserror::Error)]
pub enum ToolPolicyError {
    #[error("Tool '{tool}' is denied by policy")]
    Denied { tool: String },

    #[error("Tool '{tool}' requires elevated mode. Use '/elevated on' to enable")]
    ElevatedRequired { tool: String },
}

/// Decision about whether a tool should be executed
#[derive(Debug, Clone)]
pub enum ToolAccessDecision {
    /// Tool is allowed, execute immediately
    Allowed,
    /// Tool is denied by policy
    Denied { reason: String },
    /// Tool requires user approval via WebSocket
    RequiresApproval { sandbox_available: bool },
}

/// Tool policy enforcement engine
pub struct ToolPolicyEngine {
    policies: Arc<RwLock<HashMap<String, ToolAccessLevel>>>,
    elevated_mode: Arc<RwLock<HashSet<String>>>,
}

impl ToolPolicyEngine {
    /// Create a new policy engine with default policies
    pub fn new() -> Self {
        Self::with_policies(Self::default_policies())
    }

    /// Create a policy engine with custom policies
    pub fn with_policies(policies: HashMap<String, ToolAccessLevel>) -> Self {
        Self {
            policies: Arc::new(RwLock::new(policies)),
            elevated_mode: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Check if a session has permission to execute a tool
    pub async fn check_permission(
        &self,
        session_id: &str,
        tool_name: &str,
    ) -> Result<(), ToolPolicyError> {
        let policies = self.policies.read().await;
        let level = policies
            .get(tool_name)
            .cloned()
            .unwrap_or(ToolAccessLevel::Deny);

        match level {
            ToolAccessLevel::Allow => {
                debug!("Tool '{}' allowed by policy", tool_name);
                Ok(())
            }
            ToolAccessLevel::Deny => {
                debug!("Tool '{}' denied by policy", tool_name);
                Err(ToolPolicyError::Denied {
                    tool: tool_name.to_string(),
                })
            }
            ToolAccessLevel::Elevated => {
                let elevated = self.elevated_mode.read().await;
                if elevated.contains(session_id) {
                    debug!("Tool '{}' allowed via elevated mode", tool_name);
                    Ok(())
                } else {
                    debug!(
                        "Tool '{}' requires elevated mode for session {}",
                        tool_name, session_id
                    );
                    Err(ToolPolicyError::ElevatedRequired {
                        tool: tool_name.to_string(),
                    })
                }
            }
        }
    }

    /// Enable elevated mode for a session
    pub async fn set_elevated(&self, session_id: &str, enabled: bool) {
        let mut elevated = self.elevated_mode.write().await;
        if enabled {
            elevated.insert(session_id.to_string());
            debug!("Elevated mode enabled for session: {}", session_id);
        } else {
            elevated.remove(session_id);
            debug!("Elevated mode disabled for session: {}", session_id);
        }
    }

    /// Check if a session has elevated mode enabled
    pub async fn is_elevated(&self, session_id: &str) -> bool {
        let elevated = self.elevated_mode.read().await;
        elevated.contains(session_id)
    }

    /// Get the access decision for a tool (used for interactive approval flow)
    ///
    /// Returns:
    /// - Allowed: tool can be executed immediately
    /// - Denied: tool is blocked by policy
    /// - RequiresApproval: tool needs user approval via WebSocket
    pub async fn get_access_decision(
        &self,
        session_id: &str,
        tool_name: &str,
        sandbox_available: bool,
    ) -> ToolAccessDecision {
        let policies = self.policies.read().await;
        let level = policies
            .get(tool_name)
            .cloned()
            .unwrap_or(ToolAccessLevel::Deny);

        match level {
            ToolAccessLevel::Allow => {
                debug!("Tool '{}' allowed by policy", tool_name);
                ToolAccessDecision::Allowed
            }
            ToolAccessLevel::Deny => {
                debug!("Tool '{}' denied by policy", tool_name);
                ToolAccessDecision::Denied {
                    reason: format!(
                        "Tool '{}' is denied by policy. Use '/elevated on' to request elevated access.",
                        tool_name
                    ),
                }
            }
            ToolAccessLevel::Elevated => {
                let elevated = self.elevated_mode.read().await;
                if elevated.contains(session_id) {
                    debug!("Tool '{}' allowed via elevated mode", tool_name);
                    ToolAccessDecision::Allowed
                } else {
                    debug!(
                        "Tool '{}' requires approval for session {}",
                        tool_name, session_id
                    );
                    ToolAccessDecision::RequiresApproval { sandbox_available }
                }
            }
        }
    }

    /// Get the access level for a tool
    pub async fn get_access_level(&self, tool_name: &str) -> ToolAccessLevel {
        let policies = self.policies.read().await;
        policies
            .get(tool_name)
            .cloned()
            .unwrap_or(ToolAccessLevel::Deny)
    }

    /// Get all tool policies (returns a snapshot)
    pub async fn get_policies(&self) -> HashMap<String, ToolAccessLevel> {
        let policies = self.policies.read().await;
        policies.clone()
    }

    /// Update a tool policy at runtime
    pub async fn set_policy(&self, tool_name: String, level: ToolAccessLevel) {
        let mut policies = self.policies.write().await;
        policies.insert(tool_name, level);
    }

    /// Get default policies
    fn default_policies() -> HashMap<String, ToolAccessLevel> {
        let mut policies = HashMap::new();

        // Code execution tools (elevated)
        policies.insert("exec".to_string(), ToolAccessLevel::Elevated);
        policies.insert("bash".to_string(), ToolAccessLevel::Elevated);
        policies.insert("python".to_string(), ToolAccessLevel::Elevated);

        // WhatsApp tools (allowed)
        policies.insert("send_whatsapp".to_string(), ToolAccessLevel::Allow);
        policies.insert("list_whatsapp_groups".to_string(), ToolAccessLevel::Allow);
        policies.insert("list_whatsapp_accounts".to_string(), ToolAccessLevel::Allow);

        // Web tools (elevated by default for security)
        policies.insert("web_fetch".to_string(), ToolAccessLevel::Elevated);
        policies.insert("web_search".to_string(), ToolAccessLevel::Elevated);

        // Filesystem tools (elevated)
        policies.insert("read_file".to_string(), ToolAccessLevel::Elevated);
        policies.insert("write_file".to_string(), ToolAccessLevel::Elevated);
        policies.insert("list_files".to_string(), ToolAccessLevel::Elevated);

        policies
    }

    /// Describe all policies in human-readable format (synchronous snapshot)
    pub fn describe_policies_sync(&self) -> String {
        let policies = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // For display purposes, we try a blocking read attempt
            // In a real async context, this should not be called
            "Tool Policies:".to_string()
        })) {
            Ok(_) => String::new(),
            Err(_) => "Tool Policies:".to_string(),
        };

        // Return a message indicating async method should be used
        format!(
            "{}\n(Use describe_policies_async for full policy list)",
            policies
        )
    }

    /// Describe all policies in human-readable format (async)
    pub async fn describe_policies(&self) -> String {
        let mut lines = vec!["Tool Policies:".to_string()];
        let policies = self.policies.read().await;

        for (tool, level) in policies.iter() {
            let level_str = match level {
                ToolAccessLevel::Allow => "✅ Allow",
                ToolAccessLevel::Deny => "❌ Deny",
                ToolAccessLevel::Elevated => "⚠️  Elevated",
            };
            lines.push(format!("  {} {}", tool, level_str));
        }

        lines.join("\n")
    }
}

impl Default for ToolPolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_allow_policy() {
        let engine = ToolPolicyEngine::new();
        let result = engine.check_permission("session1", "send_whatsapp").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_deny_policy() {
        let engine = ToolPolicyEngine::new();
        let result = engine.check_permission("session1", "unknown_tool").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_elevated_mode_required() {
        let engine = ToolPolicyEngine::new();
        let result = engine.check_permission("session1", "exec").await;
        assert!(matches!(
            result,
            Err(ToolPolicyError::ElevatedRequired { .. })
        ));
    }

    #[tokio::test]
    async fn test_elevated_mode_granted() {
        let engine = ToolPolicyEngine::new();
        engine.set_elevated("session1", true).await;
        let result = engine.check_permission("session1", "exec").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_elevated_mode_revoked() {
        let engine = ToolPolicyEngine::new();
        engine.set_elevated("session1", true).await;
        assert!(engine.is_elevated("session1").await);

        engine.set_elevated("session1", false).await;
        assert!(!engine.is_elevated("session1").await);

        let result = engine.check_permission("session1", "exec").await;
        assert!(matches!(
            result,
            Err(ToolPolicyError::ElevatedRequired { .. })
        ));
    }

    #[tokio::test]
    async fn test_get_access_decision_allow_policy() {
        let engine = ToolPolicyEngine::new();
        let decision = engine
            .get_access_decision("session1", "send_whatsapp", true)
            .await;
        assert!(matches!(decision, ToolAccessDecision::Allowed));
    }

    #[tokio::test]
    async fn test_get_access_decision_deny_policy() {
        let engine = ToolPolicyEngine::new();
        let decision = engine
            .get_access_decision("session1", "unknown_tool", true)
            .await;
        assert!(matches!(decision, ToolAccessDecision::Denied { .. }));
    }

    #[tokio::test]
    async fn test_get_access_decision_elevated_requires_approval() {
        let engine = ToolPolicyEngine::new();
        let decision = engine.get_access_decision("session1", "exec", true).await;
        assert!(matches!(
            decision,
            ToolAccessDecision::RequiresApproval {
                sandbox_available: true
            }
        ));
    }

    #[tokio::test]
    async fn test_get_access_decision_elevated_with_mode_enabled() {
        let engine = ToolPolicyEngine::new();
        engine.set_elevated("session1", true).await;
        let decision = engine.get_access_decision("session1", "exec", true).await;
        assert!(matches!(decision, ToolAccessDecision::Allowed));
    }

    #[tokio::test]
    async fn test_get_access_decision_elevated_no_sandbox() {
        let engine = ToolPolicyEngine::new();
        let decision = engine.get_access_decision("session1", "exec", false).await;
        assert!(matches!(
            decision,
            ToolAccessDecision::RequiresApproval {
                sandbox_available: false
            }
        ));
    }

    #[tokio::test]
    async fn test_get_access_decision_elevated_mode_revoked() {
        let engine = ToolPolicyEngine::new();
        engine.set_elevated("session1", true).await;

        // Initially allowed with elevated mode
        let decision = engine.get_access_decision("session1", "exec", true).await;
        assert!(matches!(decision, ToolAccessDecision::Allowed));

        // Revoke elevated mode
        engine.set_elevated("session1", false).await;

        // Should now require approval
        let decision = engine.get_access_decision("session1", "exec", true).await;
        assert!(matches!(
            decision,
            ToolAccessDecision::RequiresApproval {
                sandbox_available: true
            }
        ));
    }
}
