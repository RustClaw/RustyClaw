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
        before_ctx.metadata.insert("tool_name".to_string(), serde_json::json!(name));
        before_ctx.metadata.insert("parameters".to_string(), serde_json::from_str(arguments).unwrap_or(serde_json::Value::Null));

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
            let params: whatsapp::SendWhatsAppParams = serde_json::from_str(&effective_arguments)
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
            let params: super::creator::CreateToolRequest =
                serde_json::from_str(&effective_arguments)
                    .context("Failed to parse create_tool parameters")?;
            super::creator::handle_create_tool(params).await
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

        after_ctx.metadata.insert("tool_name".to_string(), serde_json::json!(name));
        after_ctx.metadata.insert("parameters".to_string(), serde_json::from_str(&effective_arguments).unwrap_or(serde_json::Value::Null));
        after_ctx.metadata.insert("result".to_string(), serde_json::to_value(tool_result).unwrap_or(serde_json::Value::Null));
        after_ctx.metadata.insert("duration_ms".to_string(), serde_json::json!(duration_ms));

        let _ = registry.hooks.run_after_tool_call(after_ctx).await;
    }

    result_content
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
