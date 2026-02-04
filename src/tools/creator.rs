use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::skills::SkillManifest;

/// Request to create a new tool/skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateToolRequest {
    /// Tool name (alphanumeric + hyphens/underscores only)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Runtime: bash or python
    pub runtime: String,
    /// Executable body (script code)
    pub body: String,
    /// JSON Schema for parameters
    pub parameters: Value,
    /// Access policy: allow, deny, or elevated
    #[serde(default = "default_policy")]
    pub policy: String,
    /// Run in Docker sandbox
    #[serde(default)]
    pub sandbox: bool,
    /// Allow network access
    #[serde(default)]
    pub network: bool,
    /// Maximum execution time in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_policy() -> String {
    "allow".to_string()
}

fn default_timeout() -> u64 {
    30
}

impl CreateToolRequest {
    /// Validate the tool creation request
    pub fn validate(&self) -> Result<()> {
        // Name validation: alphanumeric + hyphens/underscores only
        if !self
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(anyhow!(
                "Tool name must contain only alphanumeric characters, hyphens, and underscores"
            ));
        }

        if self.name.is_empty() {
            return Err(anyhow!("Tool name cannot be empty"));
        }

        if self.name.len() > 100 {
            return Err(anyhow!("Tool name too long (max 100 characters)"));
        }

        // Description validation
        if self.description.is_empty() {
            return Err(anyhow!("Description cannot be empty"));
        }

        if self.description.len() > 500 {
            return Err(anyhow!("Description too long (max 500 characters)"));
        }

        // Body validation
        if self.body.is_empty() {
            return Err(anyhow!("Tool body cannot be empty"));
        }

        // Runtime validation
        if !["bash", "python"].contains(&self.runtime.as_str()) {
            return Err(anyhow!(
                "Invalid runtime: must be 'bash' or 'python'"
            ));
        }

        // Syntax validation based on runtime
        match self.runtime.as_str() {
            "bash" => validate_bash_syntax(&self.body)?,
            "python" => validate_python_syntax(&self.body)?,
            _ => {}
        }

        // Parameters validation (must be valid JSON)
        if self.parameters.is_null() {
            return Err(anyhow!("Parameters cannot be null"));
        }

        if !self.parameters.is_object() {
            return Err(anyhow!("Parameters must be a JSON object"));
        }

        // Verify it has at least a type field
        if self.parameters.get("type").is_none() {
            return Err(anyhow!(
                "Parameters must include a 'type' field (JSON Schema)"
            ));
        }

        // Policy validation
        if !["allow", "deny", "elevated"].contains(&self.policy.as_str()) {
            return Err(anyhow!(
                "Invalid policy: must be 'allow', 'deny', or 'elevated'"
            ));
        }

        // Timeout validation
        if self.timeout_secs == 0 || self.timeout_secs > 3600 {
            return Err(anyhow!("Timeout must be between 1 and 3600 seconds"));
        }

        Ok(())
    }

    /// Convert to SkillManifest format
    pub fn to_skill_manifest(&self) -> SkillManifest {
        SkillManifest {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: self.parameters.clone(),
            runtime: self.runtime.clone(),
            sandbox: self.sandbox,
            network: self.network,
            policy: self.policy.clone(),
            timeout_secs: self.timeout_secs,
        }
    }

    /// Generate YAML skill file content (frontmatter + body)
    pub fn to_skill_file(&self) -> String {
        let manifest_yaml = serde_yaml::to_string(&self.to_skill_manifest()).unwrap_or_default();

        format!("---\n{}---\n{}", manifest_yaml, self.body)
    }
}

// Syntax validators

fn validate_bash_syntax(body: &str) -> Result<()> {
    // Basic bash validation - check for common syntax errors
    // Allow templating with {{ }}, skip strict validation
    if body.contains("{{") && body.contains("}}") {
        return Ok(());
    }

    // Basic checks for obvious syntax issues
    if body.contains("$(") && !body.contains(")") {
        return Err(anyhow!("Bash syntax error: unmatched command substitution"));
    }

    Ok(())
}

fn validate_python_syntax(body: &str) -> Result<()> {
    // Check for basic Python syntax issues
    if body.is_empty() {
        return Err(anyhow!("Python body cannot be empty"));
    }

    // Could use Python AST parser for deeper validation
    // For now, just ensure basic structure
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_tool_request() {
        let req = CreateToolRequest {
            name: "test-tool".to_string(),
            description: "A test tool".to_string(),
            runtime: "bash".to_string(),
            body: "echo hello".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 30,
        };

        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_name_with_special_chars() {
        let req = CreateToolRequest {
            name: "test@tool#invalid".to_string(),
            description: "Test".to_string(),
            runtime: "bash".to_string(),
            body: "echo".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 30,
        };

        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_empty_name() {
        let req = CreateToolRequest {
            name: "".to_string(),
            description: "Test".to_string(),
            runtime: "bash".to_string(),
            body: "echo".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 30,
        };

        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_name_too_long() {
        let req = CreateToolRequest {
            name: "a".repeat(101),
            description: "Test".to_string(),
            runtime: "bash".to_string(),
            body: "echo".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 30,
        };

        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_empty_description() {
        let req = CreateToolRequest {
            name: "test-tool".to_string(),
            description: "".to_string(),
            runtime: "bash".to_string(),
            body: "echo".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 30,
        };

        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_description_too_long() {
        let req = CreateToolRequest {
            name: "test-tool".to_string(),
            description: "x".repeat(501),
            runtime: "bash".to_string(),
            body: "echo".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 30,
        };

        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_empty_body() {
        let req = CreateToolRequest {
            name: "test-tool".to_string(),
            description: "Test".to_string(),
            runtime: "bash".to_string(),
            body: "".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 30,
        };

        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_runtime() {
        let req = CreateToolRequest {
            name: "test-tool".to_string(),
            description: "Test".to_string(),
            runtime: "ruby".to_string(),
            body: "echo".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 30,
        };

        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_parameters_must_have_type() {
        let req = CreateToolRequest {
            name: "test-tool".to_string(),
            description: "Test".to_string(),
            runtime: "bash".to_string(),
            body: "echo".to_string(),
            parameters: serde_json::json!({"properties": {}}),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 30,
        };

        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_policy() {
        let req = CreateToolRequest {
            name: "test-tool".to_string(),
            description: "Test".to_string(),
            runtime: "bash".to_string(),
            body: "echo".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            policy: "restricted".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 30,
        };

        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_timeout_zero() {
        let req = CreateToolRequest {
            name: "test-tool".to_string(),
            description: "Test".to_string(),
            runtime: "bash".to_string(),
            body: "echo".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 0,
        };

        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_timeout_too_high() {
        let req = CreateToolRequest {
            name: "test-tool".to_string(),
            description: "Test".to_string(),
            runtime: "bash".to_string(),
            body: "echo".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 3601,
        };

        assert!(req.validate().is_err());
    }

    #[test]
    fn test_to_skill_manifest() {
        let req = CreateToolRequest {
            name: "hello-world".to_string(),
            description: "Outputs hello world".to_string(),
            runtime: "bash".to_string(),
            body: "echo hello".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 30,
        };

        let manifest = req.to_skill_manifest();
        assert_eq!(manifest.name, "hello-world");
        assert_eq!(manifest.description, "Outputs hello world");
        assert_eq!(manifest.runtime, "bash");
        assert_eq!(manifest.policy, "allow");
    }

    #[test]
    fn test_to_skill_file_format() {
        let req = CreateToolRequest {
            name: "test-tool".to_string(),
            description: "A test tool".to_string(),
            runtime: "bash".to_string(),
            body: "echo hello".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 30,
        };

        let skill_file = req.to_skill_file();

        // Should start with ---
        assert!(skill_file.starts_with("---\n"));

        // Should contain --- delimiter
        assert!(skill_file.contains("---\n"));

        // Should end with body
        assert!(skill_file.contains("echo hello"));
    }
}
