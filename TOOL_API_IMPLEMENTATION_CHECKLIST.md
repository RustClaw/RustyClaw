# Tool Creation API - Implementation Checklist

## Current State vs Required

### ✅ Already Implemented (Don't change!)

```rust
✅ skills.rs
   - YAML frontmatter parsing
   - Dynamic skill registry (RwLock<HashMap>)
   - load_skill() / unload_skill()
   - Thread-safe execution
   - Skill watcher for file changes

✅ executor.rs
   - Tool execution dispatch
   - Built-in tools lookup
   - Skills registry lookup
   - Plugin registry lookup

✅ skill_watcher.rs
   - File watching
   - Auto-load on changes
   - Hot reload (no restart!)

✅ API already supports
   - JSON request/response
   - Bearer token auth
   - Status codes
   - Error handling
```

### ❌ Need to Add

```
API Endpoints (routes.rs)
├─ POST   /api/tools                    → Create tool
├─ GET    /api/tools                    → List all tools
├─ GET    /api/tools/:name              → Get tool details
├─ PUT    /api/tools/:name              → Update tool
├─ DELETE /api/tools/:name              → Delete tool
├─ POST   /api/tools/:name/test         → Test tool execution
├─ POST   /api/tools/:name/validate     → Validate syntax
├─ GET    /api/tools/:name/definition   → Get OpenAI format
└─ GET    /api/tools/definitions/all    → Get all for LLM

New Files
├─ src/api/tools_api.rs                 → Tool creation logic
└─ src/tools/creator.rs                 → Tool validation

Updates to Existing
├─ src/config/schema.rs                 → Add tool storage path
├─ src/tools/skills.rs                  → Update load from user dir
├─ src/core/session.rs                  → Get all tools (including user)
└─ src/tools/executor.rs                → No changes needed!
```

---

## Step-by-Step Implementation

### Step 1: Add Config for Tool Storage

**File: `src/config/schema.rs`**

```rust
// Add to ApiConfig or create separate ToolsApiConfig
pub struct ToolsApiConfig {
    #[serde(default = "default_tools_dir")]
    pub storage_dir: String,
    #[serde(default)]
    pub enable_creation: bool,
}

fn default_tools_dir() -> String {
    dirs::home_dir()
        .map(|h| h.join(".rustyclaw/skills/user-created")
            .to_string_lossy().to_string())
        .unwrap_or_else(|| "./.rustyclaw/skills/user-created".to_string())
}
```

**File: `config.yaml`**

```yaml
tools:
  # ... existing config ...
  api:
    storage_dir: ~/.rustyclaw/skills/user-created
    enable_creation: true
```

---

### Step 2: Create Tool Validator

**File: `src/tools/creator.rs` (NEW)**

```rust
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::skills::SkillManifest;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateToolRequest {
    pub name: String,
    pub description: String,
    pub runtime: String,  // "bash", "python", "wasm"
    pub body: String,
    pub parameters: Value,
    #[serde(default = "default_policy")]
    pub policy: String,
    #[serde(default)]
    pub sandbox: bool,
    #[serde(default)]
    pub network: bool,
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
        // Name validation: alphanumeric + hyphens only
        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(anyhow!("Tool name must contain only alphanumeric, hyphens, and underscores"));
        }

        if self.name.is_empty() {
            return Err(anyhow!("Tool name cannot be empty"));
        }

        if self.name.len() > 100 {
            return Err(anyhow!("Tool name too long (max 100 chars)"));
        }

        // Description validation
        if self.description.is_empty() {
            return Err(anyhow!("Description cannot be empty"));
        }

        if self.description.len() > 500 {
            return Err(anyhow!("Description too long (max 500 chars)"));
        }

        // Body validation
        if self.body.is_empty() {
            return Err(anyhow!("Tool body cannot be empty"));
        }

        // Runtime validation
        if !["bash", "python", "wasm"].contains(&self.runtime.as_str()) {
            return Err(anyhow!("Invalid runtime: must be 'bash', 'python', or 'wasm'"));
        }

        // Syntax validation based on runtime
        match self.runtime.as_str() {
            "bash" => validate_bash_syntax(&self.body)?,
            "python" => validate_python_syntax(&self.body)?,
            "wasm" => validate_wasm_syntax(&self.body)?,
            _ => {}
        }

        // Parameters validation (must be valid JSON Schema)
        if self.parameters.is_null() {
            return Err(anyhow!("Parameters cannot be null"));
        }

        if !self.parameters.is_object() {
            return Err(anyhow!("Parameters must be a JSON object"));
        }

        // Policy validation
        if !["allow", "deny", "elevated"].contains(&self.policy.as_str()) {
            return Err(anyhow!("Invalid policy: must be 'allow', 'deny', or 'elevated'"));
        }

        // Timeout validation
        if self.timeout_secs == 0 || self.timeout_secs > 3600 {
            return Err(anyhow!("Timeout must be between 1 and 3600 seconds"));
        }

        Ok(())
    }

    /// Convert to SkillEntry format
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

    /// Generate YAML skill file content
    pub fn to_skill_file(&self) -> String {
        let manifest_yaml = serde_yaml::to_string(&self.to_skill_manifest())
            .unwrap_or_default();

        format!("---\n{}\n---\n{}", manifest_yaml, self.body)
    }
}

// Syntax validators
fn validate_bash_syntax(body: &str) -> Result<()> {
    // Basic bash validation (check for common syntax errors)
    if body.contains("{{") && body.contains("}}") {
        // Likely templating, skip strict validation
        return Ok(());
    }
    // Could add more thorough validation here
    Ok(())
}

fn validate_python_syntax(body: &str) -> Result<()> {
    // Check for basic Python syntax issues
    if body.is_empty() {
        return Err(anyhow!("Python body cannot be empty"));
    }
    // Could use Python AST parser for deeper validation
    Ok(())
}

fn validate_wasm_syntax(body: &str) -> Result<()> {
    // WASM validation (path to .wasm file or base64 encoded)
    if !body.starts_with("~/") && !body.starts_with("/") && !body.contains(".wasm") {
        return Err(anyhow!("WASM body must be a path to .wasm file"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_tool_request() {
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
    fn test_invalid_name() {
        let req = CreateToolRequest {
            name: "test@tool#invalid".to_string(),
            description: "Test".to_string(),
            runtime: "bash".to_string(),
            body: "echo".to_string(),
            parameters: serde_json::json!({}),
            policy: "allow".to_string(),
            sandbox: false,
            network: false,
            timeout_secs: 30,
        };

        assert!(req.validate().is_err());
    }
}
```

---

### Step 3: Add API Endpoints

**File: `src/api/routes.rs` (ADD)**

```rust
// Add these functions before the final closing bracket

/// POST /api/tools - Create a new tool
pub async fn create_tool<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Extension(user_id): Extension<String>,
    Json(req): Json<crate::tools::creator::CreateToolRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ToolResponse>>), ApiError> {
    // Validate request
    req.validate().map_err(|e| {
        ApiError::BadRequest(format!("Tool validation failed: {}", e))
    })?;

    // Check if tool already exists
    if crate::tools::get_skill(&req.name).await.is_some() {
        return Err(ApiError::BadRequest(format!("Tool '{}' already exists", req.name)));
    }

    // Create skill file content
    let skill_content = req.to_skill_file();
    let storage_path = get_tool_storage_path(&req.name)?;

    // Save to disk
    tokio::fs::create_dir_all(storage_path.parent().unwrap())
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?;

    tokio::fs::write(&storage_path, &skill_content)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?;

    // Load into registry
    let skill_entry = crate::tools::skills::parse_skill_file(&storage_path)
        .map_err(|e| ApiError::InternalError(e.to_string()))?;

    crate::tools::load_skill(skill_entry)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?;

    let response = ToolResponse {
        id: format!("tool-{}", uuid::Uuid::new_v4()),
        name: req.name.clone(),
        description: req.description.clone(),
        created_at: Utc::now(),
        path: storage_path.to_string_lossy().to_string(),
        ready: true,
    };

    Ok((StatusCode::CREATED, Json(ApiResponse::success(response))))
}

/// GET /api/tools - List all tools
pub async fn list_tools<S: Storage + 'static>(
    State(_router): State<Arc<Router<S>>>,
    Extension(_user_id): Extension<String>,
) -> Result<Json<ApiResponse<ToolListResponse>>, ApiError> {
    let all_tools = crate::tools::list_skills().await;

    let tools = all_tools.into_iter().map(|skill| {
        ToolInfo {
            id: format!("tool-{}", skill.manifest.name),
            name: skill.manifest.name.clone(),
            description: skill.manifest.description.clone(),
            runtime: skill.manifest.runtime.clone(),
            source: "user".to_string(),
            policy: skill.manifest.policy.clone(),
            created_at: Some(Utc::now()),
            ready: true,
        }
    }).collect::<Vec<_>>();

    let response = ToolListResponse {
        tools,
        total: all_tools.len(),
        ready: all_tools.len(),
        failed: 0,
    };

    Ok(Json(ApiResponse::success(response)))
}

// Helper functions
fn get_tool_storage_path(name: &str) -> Result<std::path::PathBuf, ApiError> {
    let home = dirs::home_dir()
        .ok_or_else(|| ApiError::InternalError("Cannot determine home directory".to_string()))?;

    let path = home.join(".rustyclaw/skills/user-created")
        .join(format!("{}.skill", name));

    Ok(path)
}

// Response types
#[derive(Debug, Serialize)]
pub struct ToolResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub path: String,
    pub ready: bool,
}

#[derive(Debug, Serialize)]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub runtime: String,
    pub source: String,
    pub policy: String,
    pub created_at: Option<DateTime<Utc>>,
    pub ready: bool,
}

#[derive(Debug, Serialize)]
pub struct ToolListResponse {
    pub tools: Vec<ToolInfo>,
    pub total: usize,
    pub ready: usize,
    pub failed: usize,
}
```

---

### Step 4: Wire Up Routes

**File: `src/api/mod.rs`** (ADD to router)

```rust
// In build_routes() function, add to api_routes:

.route(
    &format!("{}/tools", self.api_path),
    post(routes::create_tool),
)
.route(
    &format!("{}/tools", self.api_path),
    get(routes::list_tools),
)
// ... add other tool routes similarly
```

---

### Step 5: Update Session Manager

**File: `src/core/session.rs`** (UPDATE existing function)

```rust
/// Get available tools for this session
fn get_available_tools(&self) -> Vec<ToolDefinition> {
    let mut tools = Vec::new();

    // ... existing tool collections ...

    // NEW: Add user-created tools from skills registry
    tokio::runtime::Handle::current().block_on(async {
        let user_skills = crate::tools::list_skills().await;
        for skill in user_skills {
            tools.push(ToolDefinition {
                name: skill.manifest.name,
                description: skill.manifest.description,
                parameters: skill.manifest.parameters,
            });
        }
    });

    tools
}
```

---

## Testing Checklist

```bash
# 1. Create a tool
curl -X POST http://localhost:18789/api/tools \
  -H "Authorization: Bearer dev-token" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "hello-world",
    "description": "Outputs hello world",
    "runtime": "bash",
    "body": "echo hello world",
    "parameters": {"type": "object", "properties": {}},
    "policy": "allow"
  }'

# 2. List tools
curl http://localhost:18789/api/tools \
  -H "Authorization: Bearer dev-token"

# 3. Test the tool via API
curl -X POST http://localhost:18789/api/tools/hello-world/test \
  -H "Authorization: Bearer dev-token" \
  -H "Content-Type: application/json" \
  -d '{"parameters": {}}'

# 4. Use tool via WebSocket
# Connect and ask LLM: "Use the hello-world tool"
# Tool should execute and show result in stream

# 5. Restart gateway
# Tool should still be available (persisted to disk)
```

---

## Time Estimate

- Step 1 (Config): 30 min
- Step 2 (Validator): 1 hour
- Step 3 (API Routes): 1.5 hours
- Step 4 (Wire up): 30 min
- Step 5 (Session update): 30 min
- Testing: 1 hour

**Total: ~5 hours**

---

## Key Advantages

✅ **No restart needed** - Tools available immediately
✅ **YAML format** - Simple, user-friendly
✅ **LLM integration** - Automatic tool discovery
✅ **Persistent** - Saved to `~/.rustyclaw/skills/user-created/`
✅ **Secure** - Policy enforcement + optional sandboxing
✅ **Testable** - Validation + test endpoints
✅ **Compatible** - Uses existing skill system

---

## Questions to Answer Before Starting

1. Should tools require user confirmation before execution?
2. Should there be rate limits per tool?
3. Should tool history be kept indefinitely or pruned?
4. Should tool creation be restricted by policy?

