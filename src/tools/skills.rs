use anyhow::{anyhow, Context, Result};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Global skill registry: skill name -> SkillEntry
static SKILL_BODIES: OnceCell<Arc<RwLock<HashMap<String, SkillEntry>>>> = OnceCell::new();

/// Initialize the global skill bodies registry
fn init_skill_bodies() -> Arc<RwLock<HashMap<String, SkillEntry>>> {
    SKILL_BODIES
        .get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
        .clone()
}

/// Skill manifest parsed from YAML frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub runtime: String,
    #[serde(default)]
    pub sandbox: bool,
    #[serde(default)]
    pub network: bool,
    #[serde(default = "default_skill_policy")]
    pub policy: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_skill_policy() -> String {
    "allow".to_string()
}

fn default_timeout() -> u64 {
    30
}

/// Skill entry with manifest and executable body
#[derive(Debug, Clone)]
pub struct SkillEntry {
    pub manifest: SkillManifest,
    pub body: String,
    pub source_path: PathBuf,
}

/// Parse a skill file: frontmatter + body separated by ---
pub fn parse_skill_file(path: &std::path::Path) -> Result<SkillEntry> {
    let content =
        std::fs::read_to_string(path).context(format!("Failed to read skill file: {:?}", path))?;

    parse_skill_content(&content, path.to_path_buf())
}

/// Parse skill content from a string
fn parse_skill_content(content: &str, path: PathBuf) -> Result<SkillEntry> {
    // Split on --- delimiters
    let parts: Vec<&str> = content.splitn(3, "---").collect();

    if parts.len() < 3 {
        return Err(anyhow!(
            "Invalid skill file format: must have frontmatter between --- delimiters"
        ));
    }

    // Part 0: empty (before first ---)
    // Part 1: YAML frontmatter
    // Part 2: body (after second ---)
    let frontmatter = parts[1].trim();
    let body = parts[2].trim_start().to_string();

    // Parse YAML frontmatter
    let manifest: SkillManifest =
        serde_yaml::from_str(frontmatter).context("Failed to parse skill frontmatter as YAML")?;

    // Validate manifest
    if manifest.name.is_empty() {
        return Err(anyhow!("Skill name cannot be empty"));
    }
    if manifest.description.is_empty() {
        return Err(anyhow!("Skill description cannot be empty"));
    }

    debug!(
        "Parsed skill '{}' (runtime: {}, sandbox: {}, timeout: {}s)",
        manifest.name, manifest.runtime, manifest.sandbox, manifest.timeout_secs
    );

    Ok(SkillEntry {
        manifest,
        body,
        source_path: path,
    })
}

/// Load a skill into the registry and register its policy
pub async fn load_skill(entry: SkillEntry) -> Result<()> {
    let skill_name = entry.manifest.name.clone();
    let policy_level = entry.manifest.policy.clone();

    // Insert into registry
    let registry = init_skill_bodies();
    {
        let mut skills = registry.write().await;
        skills.insert(skill_name.clone(), entry.clone());
    }

    // Register policy
    if let Some(policy_engine) = crate::get_tool_policy_engine() {
        if let Ok(access_level) = policy_level.parse::<crate::tools::policy::ToolAccessLevel>() {
            policy_engine
                .set_policy(skill_name.clone(), access_level)
                .await;
        }
    }

    info!("Loaded skill: '{}'", skill_name);
    Ok(())
}

/// Unload a skill from the registry
pub async fn unload_skill(name: &str) -> Result<()> {
    let registry = init_skill_bodies();
    {
        let mut skills = registry.write().await;
        skills.remove(name);
    }
    info!("Unloaded skill: '{}'", name);
    Ok(())
}

/// Get a skill by name
pub async fn get_skill(name: &str) -> Option<SkillEntry> {
    let registry = init_skill_bodies();
    let skills = registry.read().await;
    skills.get(name).cloned()
}

/// List all loaded skills
pub async fn list_skills() -> Vec<SkillEntry> {
    let registry = init_skill_bodies();
    let skills = registry.read().await;
    skills.values().cloned().collect()
}

/// Execute a skill with the given arguments
pub async fn execute_skill(name: &str, arguments: &str) -> Result<String> {
    let entry = get_skill(name)
        .await
        .ok_or_else(|| anyhow!("Skill not found: {}", name))?;

    let skill = &entry.manifest;

    // Check policy
    if let Some(policy_engine) = crate::get_tool_policy_engine() {
        // For skill execution, use a placeholder session_id since skills don't have session context yet
        if let Err(e) = policy_engine
            .check_permission("_skill_executor", name)
            .await
        {
            return Err(anyhow!("Skill policy check failed: {}", e));
        }
    }

    // Set up environment
    let env_args = arguments.to_string();

    // Try to execute via sandbox if available and requested
    if skill.sandbox {
        if let Some(sandbox) = crate::get_sandbox_manager() {
            return execute_skill_in_sandbox(&sandbox, &entry, &env_args).await;
        }
    }

    // Fall back to local execution
    execute_skill_local(&entry, &env_args).await
}

/// Execute skill in local process
async fn execute_skill_local(entry: &SkillEntry, arguments: &str) -> Result<String> {
    let skill = &entry.manifest;
    let body = &entry.body;
    let timeout_secs = skill.timeout_secs;

    // Write script to a temporary file
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!("skill_{}.sh", uuid::Uuid::new_v4()));

    // For bash/sh, use the body directly
    let script = if skill.runtime == "python" {
        // For python, ensure python3 shebang or wrap
        if !body.starts_with("#!") {
            format!("#!/usr/bin/env python3\n{}", body)
        } else {
            body.to_string()
        }
    } else {
        // For bash/sh
        if !body.starts_with("#!") {
            format!("#!/bin/bash\n{}", body)
        } else {
            body.to_string()
        }
    };

    std::fs::write(&temp_file, &script).context("Failed to write skill script to temp file")?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&temp_file, perms)
            .context("Failed to set executable permission")?;
    }

    // Execute
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        tokio::process::Command::new(&temp_file)
            .env("SKILL_ARGS", arguments)
            .output(),
    )
    .await
    .context("Skill execution timed out")?
    .context("Failed to execute skill")?;

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_file);

    // Format output
    let mut result = String::new();

    if !output.stdout.is_empty() {
        result.push_str(&String::from_utf8_lossy(&output.stdout));
    }

    if !output.stderr.is_empty() {
        if !result.is_empty() {
            result.push_str("\n--- stderr ---\n");
        }
        result.push_str(&String::from_utf8_lossy(&output.stderr));
    }

    if !output.status.success() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&format!(
            "Exit code: {}",
            output.status.code().unwrap_or(-1)
        ));
    }

    if result.is_empty() {
        result = "(skill executed but produced no output)".to_string();
    }

    Ok(result)
}

/// Execute skill in sandbox
async fn execute_skill_in_sandbox(
    sandbox: &crate::SandboxManager,
    entry: &SkillEntry,
    _arguments: &str,
) -> Result<String> {
    let skill = &entry.manifest;
    let body = &entry.body;

    // Determine runtime command
    let cmd = match skill.runtime.as_str() {
        "python" => vec!["python3", "-c", body],
        "bash" | "sh" => vec!["bash", "-c", body],
        _ => return Err(anyhow!("Unsupported runtime: {}", skill.runtime)),
    };

    // Execute in sandbox (use a placeholder session for skills)
    let result = sandbox
        .execute("_skill_executor", false, &cmd)
        .await
        .context("Sandbox execution failed")?;

    // Format output
    let mut output = String::new();

    if !result.stdout.is_empty() {
        output.push_str(&result.stdout);
    }

    if !result.stderr.is_empty() {
        if !output.is_empty() {
            output.push_str("\n--- stderr ---\n");
        }
        output.push_str(&result.stderr);
    }

    if result.exit_code != 0 {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&format!("Exit code: {}", result.exit_code));
    }

    if output.is_empty() {
        output = "(skill executed but produced no output)".to_string();
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_file() {
        let content = r#"---
name: test_skill
description: "Test skill"
parameters:
  type: object
  properties:
    msg:
      type: string
runtime: bash
sandbox: false
network: false
policy: allow
timeout_secs: 10
---
echo "hello $SKILL_ARGS"
"#;

        let entry = parse_skill_content(content, PathBuf::from("/tmp/test.md")).unwrap();
        assert_eq!(entry.manifest.name, "test_skill");
        assert_eq!(entry.manifest.description, "Test skill");
        assert_eq!(entry.manifest.runtime, "bash");
        assert!(!entry.manifest.sandbox);
        assert!(!entry.manifest.network);
        assert_eq!(entry.manifest.policy, "allow");
        assert_eq!(entry.manifest.timeout_secs, 10);
        assert!(entry.body.contains("echo"));
    }

    #[test]
    fn test_skill_manifest_defaults() {
        let content = r#"---
name: minimal_skill
description: "Minimal skill"
parameters: {}
runtime: bash
---
echo test
"#;

        let entry = parse_skill_content(content, PathBuf::from("/tmp/minimal.md")).unwrap();
        assert_eq!(entry.manifest.policy, "allow");
        assert_eq!(entry.manifest.timeout_secs, 30);
        assert!(!entry.manifest.sandbox);
        assert!(!entry.manifest.network);
    }

    #[test]
    fn test_frontmatter_splitting() {
        let content = r#"---
name: split_test
description: "Test"
parameters: {}
runtime: bash
---
#!/bin/bash
echo "line 1"
echo "line 2"
"#;

        let entry = parse_skill_content(content, PathBuf::from("/tmp/split.md")).unwrap();
        assert!(entry.body.contains("line 1"));
        assert!(entry.body.contains("line 2"));
    }

    #[test]
    fn test_missing_frontmatter() {
        let content = r#"no frontmatter here"#;
        let result = parse_skill_content(content, PathBuf::from("/tmp/bad.md"));
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_name() {
        let content = r#"---
name: ""
description: "Test"
parameters: {}
runtime: bash
---
echo test
"#;

        let result = parse_skill_content(content, PathBuf::from("/tmp/empty.md"));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_skill_lifecycle() {
        let content = r#"---
name: lifecycle_test
description: "Lifecycle test"
parameters: {}
runtime: bash
policy: allow
---
echo ok
"#;

        let entry = parse_skill_content(content, PathBuf::from("/tmp/lifecycle.md")).unwrap();
        load_skill(entry.clone()).await.unwrap();

        // Verify skill is loaded
        let retrieved = get_skill("lifecycle_test").await;
        assert!(retrieved.is_some());

        // Verify it's in the list
        let skills = list_skills().await;
        assert!(skills.iter().any(|s| s.manifest.name == "lifecycle_test"));

        // Unload
        unload_skill("lifecycle_test").await.unwrap();

        // Verify it's gone
        let retrieved = get_skill("lifecycle_test").await;
        assert!(retrieved.is_none());
    }
}
