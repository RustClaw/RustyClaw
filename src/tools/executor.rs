use anyhow::{anyhow, Context, Result};
use tracing::info;

use super::whatsapp;

/// Execute a tool by name with the given arguments
pub async fn execute_tool(name: &str, arguments: &str) -> Result<String> {
    info!("Executing tool: {} with arguments: {}", name, arguments);

    match name {
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
        _ => Err(anyhow!("Unknown tool: {}", name)),
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
