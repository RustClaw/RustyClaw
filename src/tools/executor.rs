use anyhow::{anyhow, Context, Result};
use std::time::Instant;
use tracing::{debug, info, warn};

use super::execution_result::{ToolExecutionResult, ToolRetryPolicy};
use super::whatsapp;
use crate::core::ApprovalManager;

/// Execute a tool by name with the given arguments
pub async fn execute_tool(name: &str, arguments: &str) -> Result<String> {
    execute_tool_with_context(name, arguments, None, false).await
}

/// Execute a tool with session context for policy and sandbox checks
pub async fn execute_tool_with_context(
    name: &str,
    arguments: &str,
    session_id: Option<&str>,
    is_main_session: bool,
) -> Result<String> {
    info!("Executing tool: {} with arguments: {}", name, arguments);

    // Prepare tool context for hooks
    let mut metadata = std::collections::HashMap::new();
    metadata.insert(
        "is_main_session".to_string(),
        serde_json::json!(is_main_session),
    );

    let ctx = crate::plugins::traits::ToolContext {
        session_id: session_id.unwrap_or("default").to_string(),
        workspace_dir: None,
        agent_id: None,
        message_channel: None,
        sandboxed: false, // Will be determined by the tool itself
        metadata,
    };

    // Run BeforeToolCall hooks
    let mut effective_arguments = arguments.to_string();
    if let Some(registry) = crate::plugins::get_plugin_registry() {
        let mut before_ctx = ctx.clone();
        before_ctx
            .metadata
            .insert("tool_name".to_string(), serde_json::json!(name));
        before_ctx.metadata.insert(
            "parameters".to_string(),
            serde_json::from_str(arguments).unwrap_or(serde_json::Value::Null),
        );

        if let Ok(Some(modification)) = registry.hooks.run_before_tool_call(before_ctx).await {
            if let Some(block) = modification.block_tool {
                if block {
                    return Err(anyhow!(
                        "Tool execution blocked by plugin: {}",
                        modification.block_reason.unwrap_or_default()
                    ));
                }
            }
            if let Some(modified_params) = modification.modified_parameters {
                effective_arguments = modified_params.to_string();
            }
        }
    }

    let start_time = std::time::Instant::now();

    // Check tool policy if session_id is provided
    if let Some(session_id) = session_id {
        if let Some(policy) = crate::get_tool_policy_engine() {
            policy
                .check_permission(session_id, name)
                .await
                .context(format!("Tool policy check failed for tool: {}", name))?;
        }
    }

    let result_content = match name {
        "exec" => {
            let params: super::exec::ExecParams = serde_json::from_str(&effective_arguments)
                .context("Failed to parse exec parameters")?;

            if let Some(session_id) = session_id {
                if let Some(sandbox) = crate::get_sandbox_manager() {
                    super::exec::exec_command(&sandbox, session_id, is_main_session, params).await
                } else {
                    Err(anyhow!("Sandbox manager not initialized"))
                }
            } else {
                Err(anyhow!("exec tool requires session context"))
            }
        }
        "bash" => {
            let params: super::exec::BashParams = serde_json::from_str(&effective_arguments)
                .context("Failed to parse bash parameters")?;

            if let Some(session_id) = session_id {
                if let Some(sandbox) = crate::get_sandbox_manager() {
                    super::exec::exec_bash(&sandbox, session_id, is_main_session, params).await
                } else {
                    Err(anyhow!("Sandbox manager not initialized"))
                }
            } else {
                Err(anyhow!("bash tool requires session context"))
            }
        }
        "send_whatsapp" => {
            let params: whatsapp::SendWhatsAppParams =
                serde_json::from_str(&effective_arguments)
                    .context("Failed to parse send_whatsapp parameters")?;
            whatsapp::send_whatsapp(params).await
        }
        "list_whatsapp_groups" => {
            let _params: whatsapp::ListWhatsAppGroupsParams =
                serde_json::from_str(&effective_arguments)
                    .context("Failed to parse list_whatsapp_groups parameters")?;
            whatsapp::list_whatsapp_groups(_params).await
        }
        "list_whatsapp_accounts" => {
            let _params: whatsapp::ListWhatsAppAccountsParams =
                serde_json::from_str(&effective_arguments)
                    .context("Failed to parse list_whatsapp_accounts parameters")?;
            whatsapp::list_whatsapp_accounts(_params).await
        }
        "create_tool" => {
            // Tool creation is now handled through MCP (Model Context Protocol)
            Err(anyhow!(
                "Tool creation must be done through MCP. Please use the tools/create MCP method."
            ))
        }
        "delete_tool" => {
            #[derive(serde::Deserialize)]
            struct DeleteParams {
                name: String,
            }
            let params: DeleteParams = serde_json::from_str(&effective_arguments)
                .context("Failed to parse delete_tool parameters")?;
            super::creator::handle_delete_tool(params.name).await
        }
        "web_fetch" => {
            let params: super::web::WebFetchParams = serde_json::from_str(&effective_arguments)
                .context("Failed to parse web_fetch parameters")?;
            super::web::web_fetch(params).await
        }
        "web_search" => {
            let params: super::web::WebSearchParams = serde_json::from_str(&effective_arguments)
                .context("Failed to parse web_search parameters")?;
            super::web::web_search(params).await
        }
        "append_memory" | "read_today_memory" => {
            // Construct workspace from default path or context
            // For now using default path logic duplicated from default_workspace_path
            let home = dirs::home_dir().ok_or_else(|| anyhow!("Could not find home directory"))?;
            let workspace_path = home.join(".rustyclaw").join("workspace");
            let workspace = crate::config::workspace::Workspace::new(workspace_path);

            let args_json: serde_json::Value =
                serde_json::from_str(&effective_arguments).unwrap_or(serde_json::Value::Null);

            super::memory::execute_memory_tool(name, &args_json, &workspace)
                .await
                .map(|opt| opt.unwrap_or_default())
        }
        _ => {
            // Try to find in skills registry
            if super::skills::get_skill(name).await.is_some() {
                super::skills::execute_skill(name, &effective_arguments).await
            } else if let Some(registry) = crate::plugins::get_plugin_registry() {
                // Try to find in plugin registry
                if let Ok(Some(tool)) = registry.tools.get_tool(name) {
                    (tool.execute)(effective_arguments.clone())
                        .await
                        .map(|result| result.content)
                } else {
                    Err(anyhow!("Unknown tool: {}", name))
                }
            } else {
                // If not found anywhere, return unknown tool error
                Err(anyhow!("Unknown tool: {}", name))
            }
        }
    };

    let duration_ms = start_time.elapsed().as_millis() as u64;

    // Run AfterToolCall hooks
    if let Some(registry) = crate::plugins::get_plugin_registry() {
        let mut after_ctx = ctx.clone();

        let tool_result = match &result_content {
            Ok(content) => crate::plugins::traits::ToolResult {
                content: content.clone(),
                details: None,
                success: true,
            },
            Err(e) => crate::plugins::traits::ToolResult {
                content: format!("Error: {}", e),
                details: None,
                success: false,
            },
        };

        after_ctx
            .metadata
            .insert("tool_name".to_string(), serde_json::json!(name));
        after_ctx.metadata.insert(
            "parameters".to_string(),
            serde_json::from_str(&effective_arguments).unwrap_or(serde_json::Value::Null),
        );
        after_ctx.metadata.insert(
            "result".to_string(),
            serde_json::to_value(tool_result).unwrap_or(serde_json::Value::Null),
        );
        after_ctx
            .metadata
            .insert("duration_ms".to_string(), serde_json::json!(duration_ms));

        let _ = registry.hooks.run_after_tool_call(after_ctx).await;
    }

    result_content
}

/// Execute a tool with approval flow and retry mechanism
///
/// This function implements the interactive approval flow:
/// 1. Checks tool access policy
/// 2. If approval required, creates approval request and waits for user response
/// 3. Executes tool with support for sandbox selection
/// 4. On error, retries up to 10 times with exponential backoff
/// 5. Returns detailed execution result with output, errors, and timing
pub async fn execute_tool_with_approval(
    tool_name: &str,
    arguments: &str,
    session_id: &str,
    approval_manager: &ApprovalManager,
    sandbox_available: bool,
) -> ToolExecutionResult {
    let mut attempt = 1;
    let max_attempts = 10;
    let retry_policy = ToolRetryPolicy::default();

    loop {
        // First attempt: check policy and request approval if needed
        if attempt == 1 {
            // Get the tool policy decision
            if let Some(policy) = crate::get_tool_policy_engine() {
                let decision = policy
                    .get_access_decision(session_id, tool_name, sandbox_available)
                    .await;

                match decision {
                    super::policy::ToolAccessDecision::Denied { reason } => {
                        debug!("Tool execution denied: {}", reason);
                        return ToolExecutionResult::error(reason, 0, attempt, max_attempts);
                    }
                    super::policy::ToolAccessDecision::RequiresApproval {
                        sandbox_available: _,
                    } => {
                        // Create approval request
                        let request_id = approval_manager
                            .create_approval_request(
                                session_id,
                                tool_name,
                                arguments,
                                "elevated",
                                sandbox_available,
                            )
                            .await;

                        debug!(
                            "Created approval request: request_id={}, tool={}",
                            request_id, tool_name
                        );

                        // Wait for user approval (60-second timeout)
                        match approval_manager.wait_for_approval(&request_id, 60).await {
                            Some(response) => {
                                if !response.approved {
                                    debug!("Tool execution denied by user: {}", tool_name);
                                    return ToolExecutionResult::error(
                                        "Tool execution denied by user".to_string(),
                                        0,
                                        attempt,
                                        max_attempts,
                                    );
                                }
                                debug!(
                                    "Tool execution approved: sandbox={}, remember={}",
                                    response.use_sandbox, response.remember_for_session
                                );
                            }
                            None => {
                                debug!("Tool approval request timed out after 60s: {}", tool_name);
                                return ToolExecutionResult::error(
                                    "Tool approval request timed out".to_string(),
                                    0,
                                    attempt,
                                    max_attempts,
                                );
                            }
                        }
                    }
                    super::policy::ToolAccessDecision::Allowed => {
                        debug!("Tool execution allowed by policy: {}", tool_name);
                    }
                }
            }
        }

        // Execute the tool
        let start_time = Instant::now();
        let execution_result =
            execute_tool_with_context(tool_name, arguments, Some(session_id), false).await;
        let duration_ms = start_time.elapsed().as_millis() as u64;

        match execution_result {
            Ok(output) => {
                debug!(
                    "Tool executed successfully (attempt {}/{}): {}",
                    attempt, max_attempts, tool_name
                );
                return ToolExecutionResult::success(output, duration_ms, attempt, max_attempts);
            }
            Err(e) => {
                let error_msg = format!("{}", e);

                // Check if we should retry
                if retry_policy.should_retry(attempt, true) {
                    let backoff = retry_policy.get_backoff(attempt);
                    warn!(
                        "Tool execution failed (attempt {}/{}), retrying in {:?}: {} - {}",
                        attempt, max_attempts, backoff, tool_name, error_msg
                    );

                    // Sleep for exponential backoff
                    tokio::time::sleep(backoff).await;

                    attempt += 1;
                    continue;
                } else {
                    // Max attempts reached
                    warn!(
                        "Tool execution failed after {} attempts: {} - {}",
                        attempt, tool_name, error_msg
                    );
                    return ToolExecutionResult::error(
                        error_msg,
                        duration_ms,
                        attempt,
                        max_attempts,
                    );
                }
            }
        }
    }
}

/// Convert tool execution results to a message for the LLM
pub fn format_tool_result(tool_name: &str, result: &str, success: bool) -> String {
    if success {
        format!("Tool {} executed successfully: {}", tool_name, result)
    } else {
        format!("Tool {} failed: {}", tool_name, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_unknown_tool() {
        let result = execute_tool("unknown_tool", "{}").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_format_tool_result_success() {
        let result = format_tool_result("test_tool", "Success message", true);
        assert!(result.contains("test_tool"));
        assert!(result.contains("Success message"));
    }

    #[tokio::test]
    async fn test_format_tool_result_failure() {
        let result = format_tool_result("test_tool", "Error message", false);
        assert!(result.contains("test_tool"));
        assert!(result.contains("Error message"));
    }

    #[tokio::test]
    async fn test_tool_execution_result_success() {
        let result = ToolExecutionResult::success("output".to_string(), 1000, 1, 10);
        assert!(result.is_success());
        assert!(!result.is_error());
        assert_eq!(result.status, "done");
        assert_eq!(result.attempt, 1);
    }

    #[tokio::test]
    async fn test_tool_execution_result_error() {
        let result = ToolExecutionResult::error("failed".to_string(), 500, 1, 10);
        assert!(!result.is_success());
        assert!(result.is_error());
        assert_eq!(result.status, "error");
        assert_eq!(result.attempt, 1);
        assert!(result.can_retry());
    }

    #[tokio::test]
    async fn test_tool_execution_result_max_attempts() {
        let result = ToolExecutionResult::error("failed".to_string(), 500, 10, 10);
        assert!(result.is_error());
        assert!(!result.can_retry()); // No more attempts
    }

    #[tokio::test]
    async fn test_retry_policy_backoff_progression() {
        let policy = ToolRetryPolicy::default();

        // Verify exponential backoff progression
        assert_eq!(policy.get_backoff(1).as_millis(), 100);
        assert_eq!(policy.get_backoff(2).as_millis(), 200);
        assert_eq!(policy.get_backoff(3).as_millis(), 400);
        assert_eq!(policy.get_backoff(4).as_millis(), 800);
        assert_eq!(policy.get_backoff(5).as_millis(), 1600);
        assert_eq!(policy.get_backoff(6).as_millis(), 3200);
        // Capped at 5000ms
        assert_eq!(policy.get_backoff(7).as_millis(), 5000);
        assert_eq!(policy.get_backoff(8).as_millis(), 5000);
    }

    #[tokio::test]
    async fn test_retry_policy_max_retries_enforcement() {
        let policy = ToolRetryPolicy::default();

        // Should retry up to 10 times
        for attempt in 1..10 {
            assert!(policy.should_retry(attempt, true));
        }

        // Should NOT retry on attempt 10
        assert!(!policy.should_retry(10, true));

        // Should NOT retry on success
        assert!(!policy.should_retry(1, false));
    }

    #[tokio::test]
    async fn test_retry_policy_custom_max_retries() {
        let policy = ToolRetryPolicy::with_max_retries(3);

        assert_eq!(policy.max_retries, 3);
        assert!(policy.should_retry(1, true)); // Can retry on attempt 1
        assert!(policy.should_retry(2, true)); // Can retry on attempt 2
        assert!(!policy.should_retry(3, true)); // Cannot retry on attempt 3 (at max)
        assert!(!policy.should_retry(4, true)); // Cannot retry on attempt 4 (exceeds max)
    }
}
