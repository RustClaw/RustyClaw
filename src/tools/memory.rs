use crate::config::workspace::Workspace;
use crate::core::memory::MemoryManager;
use crate::llm::ToolDefinition;
use anyhow::Result;
use serde_json::json;

pub fn get_memory_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "append_memory".to_string(),
            description: "Append a significant event or fact to the daily memory log.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The content to remember"
                    }
                },
                "required": ["content"]
            }),
        },
        ToolDefinition {
            name: "read_today_memory".to_string(),
            description: "Read the full daily memory log for today.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
            }),
        },
    ]
}

pub async fn execute_memory_tool(
    name: &str,
    args: &serde_json::Value,
    workspace: &Workspace,
) -> Result<Option<String>> {
    let memory_manager = MemoryManager::new(workspace.path());

    match name {
        "append_memory" => {
            let content = args["content"].as_str().unwrap_or_default();
            if content.trim().is_empty() {
                return Ok(Some("Error: content cannot be empty".to_string()));
            }
            memory_manager.append_memory(content)?;
            Ok(Some("Memory appended successfully.".to_string()))
        }
        "read_today_memory" => {
            let content = memory_manager.get_today_log()?;
            if content.is_empty() {
                Ok(Some("No memory logged for today yet.".to_string()))
            } else {
                Ok(Some(content))
            }
        }
        _ => Ok(None),
    }
}
