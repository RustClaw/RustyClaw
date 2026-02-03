use anyhow::{anyhow, Context, Result};
use tracing::info;

use super::whatsapp;

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

    // Check tool policy if session_id is provided
    if let Some(session_id) = session_id {
        if let Some(policy) = crate::get_tool_policy_engine() {
            policy
                .check_permission(session_id, name)
                .await
                .context(format!("Tool policy check failed for tool: {}", name))?;
        }
    }

    match name {
        "exec" => {
            let params: super::exec::ExecParams =
                serde_json::from_str(arguments).context("Failed to parse exec parameters")?;

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
            let params: super::exec::BashParams =
                serde_json::from_str(arguments).context("Failed to parse bash parameters")?;

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
            let params: whatsapp::SendWhatsAppParams = serde_json::from_str(arguments)
                .context("Failed to parse send_whatsapp parameters")?;
            whatsapp::send_whatsapp(params).await
        }
        "list_whatsapp_groups" => {
            let _params: whatsapp::ListWhatsAppGroupsParams = serde_json::from_str(arguments)
                .context("Failed to parse list_whatsapp_groups parameters")?;
            whatsapp::list_whatsapp_groups(_params).await
        }
        "list_whatsapp_accounts" => {
            let _params: whatsapp::ListWhatsAppAccountsParams = serde_json::from_str(arguments)
                .context("Failed to parse list_whatsapp_accounts parameters")?;
            whatsapp::list_whatsapp_accounts(_params).await
        }
        _ => {
            // Try to find in skills registry
            if super::skills::get_skill(name).await.is_some() {
                return super::skills::execute_skill(name, arguments).await;
            }

            // Try to find in plugin registry
            if let Some(registry) = crate::plugins::get_plugin_registry() {
                if let Ok(Some(tool)) = registry.tools.get_tool(name) {
                    return (tool.execute)(arguments.to_string())
                        .await
                        .map(|result| result.content);
                }
            }

            // If not found anywhere, return unknown tool error
            Err(anyhow!("Unknown tool: {}", name))
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
}
