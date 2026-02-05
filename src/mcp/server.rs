use super::types::*;
use crate::tools::{
    exec::get_exec_tool_definitions, skills::list_skills, web::get_web_tool_definitions,
    whatsapp::get_whatsapp_tool_definitions,
};
use serde_json::Value;
use tracing::{error, info};

pub struct McpServer;

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    // ... existing methods ...
    pub fn new() -> Self {
        Self
    }

    pub async fn handle_request(&self, req: JsonRpcRequest) -> JsonRpcResponse {
        let id = req.id.clone();
        match req.method.as_str() {
            "initialize" => self.handle_initialize(id, req.params),
            "notifications/initialized" => {
                // Client is ready
                JsonRpcResponse::success(id, Value::Null)
            }
            "ping" => JsonRpcResponse::success(id, Value::Null),
            "tools/list" => self.handle_list_tools(id).await,
            "tools/call" => self.handle_call_tool(id, req.params).await,
            _ => JsonRpcResponse::error(id, -32601, format!("Method not found: {}", req.method)),
        }
    }

    fn handle_initialize(&self, id: Option<Value>, _params: Option<Value>) -> JsonRpcResponse {
        let result = InitializeResult {
            protocol_version: "2024-11-05".to_string(), // Latest stable draft
            capabilities: ServerCapabilities {
                logging: Some(serde_json::json!({})),
                prompts: None,
                resources: None,
                tools: Some(serde_json::json!({
                    "listChanged": true
                })),
            },
            server_info: ServerInfo {
                name: "rustyclaw-gateway".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
    }

    async fn handle_list_tools(&self, id: Option<Value>) -> JsonRpcResponse {
        let mut tools = Vec::new();

        // 1. WhatsApp Tools
        for def in get_whatsapp_tool_definitions() {
            tools.push(Tool {
                name: def.name,
                description: Some(def.description),
                input_schema: def.parameters,
            });
        }

        // 2. Exec Tools
        for def in get_exec_tool_definitions() {
            if let Some(obj) = def.as_object() {
                // Exec tools format is slightly different in get_exec_tool_definitions (OpenAI format)
                // It returns { type: "function", function: { ... } }
                if let Some(func) = obj.get("function") {
                    if let Ok(tool) = serde_json::from_value::<Tool>(func.clone()) {
                        tools.push(tool);
                    } else {
                        // Manual mapping if struct doesn't match exactly
                        let name = func["name"].as_str().unwrap_or("").to_string();
                        let description = func["description"].as_str().map(|s| s.to_string());
                        let input_schema = func["parameters"].clone();
                        tools.push(Tool {
                            name,
                            description,
                            input_schema,
                        });
                    }
                }
            }
        }

        // 3. Web Tools
        for def in get_web_tool_definitions() {
            if let Some(obj) = def.as_object() {
                if let Some(func) = obj.get("function") {
                    let name = func["name"].as_str().unwrap_or("").to_string();
                    let description = func["description"].as_str().map(|s| s.to_string());
                    let input_schema = func["parameters"].clone();
                    tools.push(Tool {
                        name,
                        description,
                        input_schema,
                    });
                }
            }
        }

        // 4. Skills
        for skill in list_skills().await {
            tools.push(Tool {
                name: skill.manifest.name,
                description: Some(skill.manifest.description),
                input_schema: skill.manifest.parameters,
            });
        }

        // 4. Core Creation Tools
        tools.push(Tool {
            name: "create_tool".to_string(),
            description: Some("Create a new persistent tool/capability for yourself. This tool will be saved and available in future sessions. You can write the tool logic in Bash or Python.".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Unique name for the tool (alphanumeric + underscores/hyphens)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Clear description of what the tool does and how to use it"
                    },
                    "runtime": {
                        "type": "string",
                        "enum": ["bash", "python"],
                        "description": "The runtime to use for the tool"
                    },
                    "body": {
                        "type": "string",
                        "description": "The executable script content"
                    },
                    "parameters": {
                        "type": "object",
                        "description": "JSON Schema for the tool's input parameters. Must include a 'type' field (usually 'object') and 'properties'."
                    },
                    "policy": {
                        "type": "string",
                        "enum": ["allow", "elevated"],
                        "description": "Access policy. 'allow' for standard tools, 'elevated' for dangerous tools.",
                        "default": "allow"
                    },
                    "sandbox": {
                        "type": "boolean",
                        "description": "Whether to run the tool in a Docker sandbox",
                        "default": true
                    }
                },
                "required": ["name", "description", "runtime", "body", "parameters"]
            }),
        });

        // 5. Plugins
        if let Some(registry) = crate::plugins::get_plugin_registry() {
            match registry.tools.list_tools() {
                Ok(names) => {
                    for name in names {
                        if let Ok(Some(tool)) = registry.tools.get_tool(&name) {
                            tools.push(Tool {
                                name: tool.name,
                                description: Some(tool.description),
                                input_schema: tool.parameters,
                            });
                        }
                    }
                }
                Err(e) => error!("Failed to list plugin tools: {}", e),
            }
        }

        let result = ListToolsResult {
            tools,
            next_cursor: None,
        };

        JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
    }

    async fn handle_call_tool(&self, id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
        let params: CallToolParams = match serde_json::from_value(params.unwrap_or(Value::Null)) {
            Ok(p) => p,
            Err(e) => return JsonRpcResponse::error(id, -32602, format!("Invalid params: {}", e)),
        };

        info!("MCP Tool Call: {}", params.name);

        // Convert arguments to JSON string for execute_tool
        let args_str = params
            .arguments
            .map(|a| a.to_string())
            .unwrap_or_else(|| "{}".to_string());

        // We don't have a specific session ID here for MCP.
        // We can create a temporary or "mcp" session ID, or pass None if allowed.
        // execute_tool_with_context checks policy.
        // We'll use "mcp-client" as session_id.
        // Note: Policy engine needs to allow this session ID or we need to add a "mcp" policy bypass/check.
        // For now, let's pass a known session ID.
        let session_id = "mcp-session";

        match crate::tools::executor::execute_tool_with_context(
            &params.name,
            &args_str,
            Some(session_id),
            false,
        )
        .await
        {
            Ok(result) => {
                let content = vec![ToolContent::Text { text: result }];
                let call_result = CallToolResult {
                    content,
                    is_error: false,
                };
                JsonRpcResponse::success(id, serde_json::to_value(call_result).unwrap())
            }
            Err(e) => {
                let content = vec![ToolContent::Text {
                    text: format!("Error: {}", e),
                }];
                let call_result = CallToolResult {
                    content,
                    is_error: true,
                };
                JsonRpcResponse::success(id, serde_json::to_value(call_result).unwrap())
            }
        }
    }
}
