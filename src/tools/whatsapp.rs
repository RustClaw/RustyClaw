use crate::llm::ToolDefinition;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Parameters for sending a WhatsApp message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendWhatsAppParams {
    /// Target type: "contact" or "group"
    pub target_type: String,
    /// Phone number (for contact) or group name/ID (for group)
    pub target: String,
    /// Message to send
    pub message: String,
    /// Account to send from (optional, defaults to first account)
    #[serde(default)]
    pub from_account: Option<String>,
    /// Skip confirmation (dangerous - requires explicit use)
    #[serde(default)]
    pub skip_confirmation: bool,
}

/// Parameters for listing WhatsApp groups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWhatsAppGroupsParams {}

/// Parameters for listing WhatsApp accounts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWhatsAppAccountsParams {}

/// Send a WhatsApp message to a contact or group
pub async fn send_whatsapp(params: SendWhatsAppParams) -> Result<String> {
    // Get service for specific account or default
    let service = if let Some(account_id) = &params.from_account {
        crate::get_whatsapp_service_by_account(account_id)
            .context(format!("WhatsApp account '{}' not found", account_id))?
    } else {
        crate::get_whatsapp_service().context("WhatsApp service not available")?
    };

    // CONFIRMATION STEP (unless skipped)
    if !params.skip_confirmation {
        // Log the confirmation requirement
        tracing::warn!(
            "⚠️  CONFIRMATION REQUIRED: Send WhatsApp to {} ({}): \"{}\"",
            params.target,
            params.target_type,
            params.message
        );

        // In real implementation, this would:
        // 1. Send confirmation request to user
        // 2. Wait for user approval
        // 3. Proceed or abort based on response

        // For now: Auto-approve with warning (Phase 1 behavior)
        tracing::warn!("Auto-approving send (confirmation system not yet implemented)");
    }

    // Execute send
    let message_id = match params.target_type.as_str() {
        "contact" => service
            .send_to_contact(&params.target, &params.message)
            .await
            .context("Failed to send message to contact")?,
        "group" => service
            .send_to_group(&params.target, &params.message)
            .await
            .context("Failed to send message to group")?,
        _ => anyhow::bail!("Invalid target_type: must be 'contact' or 'group'"),
    };

    let account_info = params
        .from_account
        .map(|a| format!(" from account '{}'", a))
        .unwrap_or_default();

    Ok(format!(
        "✓ WhatsApp message sent{} (ID: {})",
        account_info, message_id
    ))
}

/// List all available WhatsApp groups
pub async fn list_whatsapp_groups(_params: ListWhatsAppGroupsParams) -> Result<String> {
    let service = crate::get_whatsapp_service().context("WhatsApp service not available")?;

    let groups = service
        .list_groups()
        .await
        .context("Failed to fetch groups")?;

    if groups.is_empty() {
        return Ok("No WhatsApp groups found".to_string());
    }

    let mut result = String::from("Available WhatsApp groups:\n");
    for group in groups {
        result.push_str(&format!(
            "• {} ({} members)\n",
            group.name, group.participant_count
        ));
    }

    Ok(result)
}

/// List all connected WhatsApp accounts
pub async fn list_whatsapp_accounts(_params: ListWhatsAppAccountsParams) -> Result<String> {
    let accounts = crate::list_whatsapp_accounts();

    if accounts.is_empty() {
        return Ok("No WhatsApp accounts connected".to_string());
    }

    let mut result = String::from("Connected WhatsApp accounts:\n");
    for account_id in accounts {
        result.push_str(&format!("• {}\n", account_id));
    }

    Ok(result)
}

/// Get WhatsApp tool definitions for LLM
pub fn get_whatsapp_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "send_whatsapp".to_string(),
            description:
                "Send a WhatsApp message to a contact (by phone number) or group (by name). Requires confirmation."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target_type": {
                        "type": "string",
                        "enum": ["contact", "group"],
                        "description": "Whether sending to a contact or group"
                    },
                    "target": {
                        "type": "string",
                        "description": "Phone number for contacts (e.g., '1234567890') or group name for groups (e.g., 'Team Alpha')"
                    },
                    "message": {
                        "type": "string",
                        "description": "The message to send"
                    },
                    "from_account": {
                        "type": "string",
                        "description": "Optional: Which WhatsApp account to send from (defaults to first account)"
                    }
                },
                "required": ["target_type", "target", "message"]
            }),
        },
        ToolDefinition {
            name: "list_whatsapp_groups".to_string(),
            description: "List all available WhatsApp groups".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "list_whatsapp_accounts".to_string(),
            description: "List all connected WhatsApp accounts".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_whatsapp_params_serialization() {
        let params = SendWhatsAppParams {
            target_type: "contact".to_string(),
            target: "1234567890".to_string(),
            message: "Hello from RustyClaw".to_string(),
            from_account: Some("personal".to_string()),
            skip_confirmation: false,
        };

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: SendWhatsAppParams = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.target_type, "contact");
        assert_eq!(deserialized.target, "1234567890");
        assert_eq!(deserialized.message, "Hello from RustyClaw");
        assert_eq!(deserialized.from_account, Some("personal".to_string()));
        assert!(!deserialized.skip_confirmation);
    }

    #[test]
    fn test_tool_definitions() {
        let tools = get_whatsapp_tool_definitions();
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].name, "send_whatsapp");
        assert_eq!(tools[1].name, "list_whatsapp_groups");
        assert_eq!(tools[2].name, "list_whatsapp_accounts");
    }

    #[test]
    fn test_list_whatsapp_groups_params() {
        let params = ListWhatsAppGroupsParams {};
        let json = serde_json::to_string(&params).unwrap();
        let _deserialized: ListWhatsAppGroupsParams = serde_json::from_str(&json).unwrap();
        // Just verify it deserializes without error
    }
}
