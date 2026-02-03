use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::sandbox::SandboxManager;

/// Parameters for the exec tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecParams {
    /// The command to execute
    pub command: String,

    /// Command arguments
    #[serde(default)]
    pub args: Vec<String>,

    /// Working directory (optional)
    #[serde(default)]
    pub working_dir: Option<String>,
}

/// Parameters for the bash tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashParams {
    /// The bash script to execute
    pub script: String,
}

/// Execute a command in the sandbox
pub async fn exec_command(
    sandbox: &SandboxManager,
    session_id: &str,
    is_main_session: bool,
    params: ExecParams,
) -> Result<String> {
    // Prepare command array
    let mut cmd = vec![params.command.clone()];
    cmd.extend(params.args.clone());

    // Convert to &str references
    let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();

    // Execute with sandboxing
    let result = sandbox
        .execute(session_id, is_main_session, &cmd_refs)
        .await?;

    // Format output
    let mut output = String::new();

    if !result.stdout.is_empty() {
        output.push_str("Output:\n");
        output.push_str(&result.stdout);
        if !result.stdout.ends_with('\n') {
            output.push('\n');
        }
    }

    if !result.stderr.is_empty() {
        output.push_str("Errors:\n");
        output.push_str(&result.stderr);
        if !result.stderr.ends_with('\n') {
            output.push('\n');
        }
    }

    output.push_str(&format!("Exit code: {}", result.exit_code));

    Ok(output)
}

/// Execute a bash script in the sandbox
pub async fn exec_bash(
    sandbox: &SandboxManager,
    session_id: &str,
    is_main_session: bool,
    params: BashParams,
) -> Result<String> {
    let result = sandbox
        .execute(session_id, is_main_session, &["bash", "-c", &params.script])
        .await?;

    let mut output = String::new();

    if !result.stdout.is_empty() {
        output.push_str(&result.stdout);
        if !result.stdout.ends_with('\n') {
            output.push('\n');
        }
    }

    if !result.stderr.is_empty() {
        if !output.is_empty() {
            output.push_str("stderr:\n");
        }
        output.push_str(&result.stderr);
        if !result.stderr.ends_with('\n') {
            output.push('\n');
        }
    }

    if result.exit_code != 0 {
        output.push_str(&format!("(exit code: {})", result.exit_code));
    }

    Ok(output)
}

/// Get tool definitions for code execution tools
pub fn get_exec_tool_definitions() -> Vec<serde_json::Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "exec",
                "description": "Execute a command in the sandbox. Requires elevated mode.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The command to execute"
                        },
                        "args": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Command arguments",
                            "default": []
                        },
                        "working_dir": {
                            "type": "string",
                            "description": "Working directory (optional)"
                        }
                    },
                    "required": ["command"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "bash",
                "description": "Execute a bash script in the sandbox. Requires elevated mode.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "script": {
                            "type": "string",
                            "description": "The bash script to execute"
                        }
                    },
                    "required": ["script"]
                }
            }
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exec_params_serialization() {
        let params = ExecParams {
            command: "echo".to_string(),
            args: vec!["hello".to_string(), "world".to_string()],
            working_dir: None,
        };

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: ExecParams = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.command, "echo");
        assert_eq!(deserialized.args, vec!["hello", "world"]);
    }

    #[test]
    fn test_bash_params_serialization() {
        let params = BashParams {
            script: "echo 'hello'".to_string(),
        };

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: BashParams = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.script, "echo 'hello'");
    }

    #[test]
    fn test_tool_definitions() {
        let defs = get_exec_tool_definitions();
        assert_eq!(defs.len(), 2);

        // Check exec tool
        assert_eq!(defs[0]["function"]["name"], "exec");

        // Check bash tool
        assert_eq!(defs[1]["function"]["name"], "bash");
    }
}
