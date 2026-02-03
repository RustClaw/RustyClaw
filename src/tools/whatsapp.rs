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
}

/// Parameters for listing WhatsApp groups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWhatsAppGroupsParams {}

/// Send a WhatsApp message to a contact or group
pub async fn send_whatsapp(params: SendWhatsAppParams) -> Result<String> {
    let service = crate::get_whatsapp_service().context("WhatsApp service not available")?;

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

    Ok(format!(
        "✓ WhatsApp message sent successfully (ID: {})",
        message_id
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

/// Get WhatsApp tool definitions for LLM
pub fn get_whatsapp_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "send_whatsapp".to_string(),
            description:
                "Send a WhatsApp message to a contact (by phone number) or group (by name)"
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
        };

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: SendWhatsAppParams = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.target_type, "contact");
        assert_eq!(deserialized.target, "1234567890");
        assert_eq!(deserialized.message, "Hello from RustyClaw");
    }

    #[test]
    fn test_tool_definitions() {
        let tools = get_whatsapp_tool_definitions();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "send_whatsapp");
        assert_eq!(tools[1].name, "list_whatsapp_groups");
    }

    #[test]
    fn test_list_whatsapp_groups_params() {
        let params = ListWhatsAppGroupsParams {};
        let json = serde_json::to_string(&params).unwrap();
        let deserialized: ListWhatsAppGroupsParams = serde_json::from_str(&json).unwrap();
        // Just verify it deserializes without error
        drop(deserialized);
    }
}
