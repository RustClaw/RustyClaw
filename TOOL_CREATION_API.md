# Tool Creation API - Complete Design Document

**Status:** Implementation Ready
**Priority:** HIGH
**Impact:** Enables dynamic tool creation without restart

---

## Overview

RustyClaw will support **dynamic tool creation via REST API** with the following design:

### Key Features
- ✅ YAML-based tool definition (already in system)
- ✅ Dynamic loading (no restart needed)
- ✅ LLM auto-discovery of tools
- ✅ Tool persistence to disk
- ✅ Tool versioning
- ✅ Tool validation

---

## Architecture

### Tool Storage

```
~/.rustyclaw/
├── skills/
│   ├── my-tool-1.skill
│   ├── my-tool-2.skill
│   └── user-created/
│       ├── calculator.skill
│       ├── weather-api.skill
│       └── slack-notifier.skill
├── tools.json (registry metadata)
└── tool-history/ (for versioning)
```

### Tool Lifecycle

```
1. Create (POST /api/tools)
   ↓
2. Validate (syntax, parameters, syntax)
   ↓
3. Save to disk (~/.rustyclaw/skills/user-created/)
   ↓
4. Load into registry (RwLock<HashMap>)
   ↓
5. Register policy
   ↓
6. Available to LLM immediately (no restart!)
```

---

## API Endpoints

### 1. Create Tool

```
POST /api/tools
Authorization: Bearer token
Content-Type: application/json
```

**Request:**
```json
{
  "name": "weather-check",
  "description": "Check current weather for a location",
  "runtime": "bash",
  "body": "curl -s 'https://api.weather.example.com/current?city=$city'",
  "parameters": {
    "type": "object",
    "properties": {
      "city": {
        "type": "string",
        "description": "City name to check weather for"
      }
    },
    "required": ["city"]
  },
  "policy": "elevated",
  "sandbox": false,
  "network": true,
  "timeout_secs": 30
}
```

**Response (201):**
```json
{
  "status": "success",
  "data": {
    "id": "tool-550e8400-e29b-41d4-a716-446655440000",
    "name": "weather-check",
    "description": "Check current weather for a location",
    "created_at": "2026-02-03T12:34:56Z",
    "path": "~/.rustyclaw/skills/user-created/weather-check.skill",
    "ready": true
  }
}
```

---

### 2. List Tools

```
GET /api/tools
Authorization: Bearer token
```

**Query Parameters:**
- `filter`: "built-in", "skills", "plugins", "all" (default: "all")
- `runtime`: filter by runtime ("bash", "python", "wasm")

**Response:**
```json
{
  "status": "success",
  "data": {
    "tools": [
      {
        "id": "tool-builtin-exec",
        "name": "exec",
        "description": "Execute shell commands",
        "runtime": "built-in",
        "source": "built-in",
        "policy": "elevated",
        "created_at": null,
        "ready": true
      },
      {
        "id": "tool-builtin-bash",
        "name": "bash",
        "description": "Execute bash commands",
        "runtime": "built-in",
        "source": "built-in",
        "policy": "elevated",
        "created_at": null,
        "ready": true
      },
      {
        "id": "tool-550e8400-e29b-41d4-a716-446655440000",
        "name": "weather-check",
        "description": "Check current weather for a location",
        "runtime": "bash",
        "source": "user",
        "policy": "elevated",
        "created_at": "2026-02-03T12:34:56Z",
        "ready": true
      }
    ],
    "total": 3,
    "ready": 3,
    "failed": 0
  }
}
```

---

### 3. Get Tool Details

```
GET /api/tools/:name
Authorization: Bearer token
```

**Response:**
```json
{
  "status": "success",
  "data": {
    "id": "tool-550e8400-e29b-41d4-a716-446655440000",
    "name": "weather-check",
    "description": "Check current weather for a location",
    "runtime": "bash",
    "body": "curl -s 'https://api.weather.example.com/current?city=$city'",
    "parameters": {
      "type": "object",
      "properties": {
        "city": {
          "type": "string",
          "description": "City name"
        }
      },
      "required": ["city"]
    },
    "policy": "elevated",
    "sandbox": false,
    "network": true,
    "timeout_secs": 30,
    "created_at": "2026-02-03T12:34:56Z",
    "path": "~/.rustyclaw/skills/user-created/weather-check.skill",
    "ready": true
  }
}
```

---

### 4. Update Tool

```
PUT /api/tools/:name
Authorization: Bearer token
Content-Type: application/json
```

**Request:** (same fields as create)

**Response:** Updated tool object

---

### 5. Delete Tool

```
DELETE /api/tools/:name
Authorization: Bearer token
```

**Query Parameters:**
- `keep_backup`: "true" (default: true) - save to tool-history/

**Response:**
```json
{
  "status": "success",
  "data": {
    "message": "Tool 'weather-check' deleted",
    "backup_path": "~/.rustyclaw/tool-history/weather-check.skill.bak"
  }
}
```

---

### 6. Test Tool

```
POST /api/tools/:name/test
Authorization: Bearer token
Content-Type: application/json
```

**Request:**
```json
{
  "parameters": {
    "city": "San Francisco"
  }
}
```

**Response:**
```json
{
  "status": "success",
  "data": {
    "output": "Current weather in San Francisco: 72°F",
    "execution_time_ms": 245,
    "status": "success"
  }
}
```

---

### 7. Test Tool (Dry-run)

```
POST /api/tools/:name/validate
Authorization: Bearer token
Content-Type: application/json
```

**Request:**
```json
{
  "check_syntax": true,
  "check_parameters": true
}
```

**Response:**
```json
{
  "status": "success",
  "data": {
    "valid": true,
    "errors": [],
    "warnings": []
  }
}
```

---

### 8. Get Tool Definition (For LLM)

```
GET /api/tools/:name/definition
Authorization: Bearer token
```

**Response (OpenAI tool format):**
```json
{
  "status": "success",
  "data": {
    "type": "function",
    "function": {
      "name": "weather-check",
      "description": "Check current weather for a location",
      "parameters": {
        "type": "object",
        "properties": {
          "city": {
            "type": "string",
            "description": "City name"
          }
        },
        "required": ["city"]
      }
    }
  }
}
```

---

### 9. Get All Tool Definitions (For LLM)

```
GET /api/tools/definitions/all
Authorization: Bearer token
```

**Response:** Array of tool definitions (for LLM to understand available tools)

```json
{
  "status": "success",
  "data": [
    {
      "type": "function",
      "function": {
        "name": "weather-check",
        "description": "...",
        "parameters": { ... }
      }
    },
    ...
  ]
}
```

---

## Tool Definition Format (YAML Skill File)

### File: `~/.rustyclaw/skills/user-created/weather-check.skill`

```yaml
---
name: weather-check
description: Check current weather for a location
runtime: bash
parameters:
  type: object
  properties:
    city:
      type: string
      description: City name to check weather for
  required:
    - city
policy: elevated
sandbox: false
network: true
timeout_secs: 30
---
curl -s "https://api.weather.example.com/current?city=${city}"
```

### Fields Explained

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | required | Unique tool identifier (alphanumeric + hyphens) |
| `description` | string | required | What this tool does (shown to LLM) |
| `runtime` | string | required | `bash`, `python`, or `wasm` |
| `parameters` | JSON Schema | required | Tool input schema |
| `policy` | string | "allow" | `allow`, `deny`, or `elevated` |
| `sandbox` | boolean | false | Run in Docker sandbox |
| `network` | boolean | false | Allow network access |
| `timeout_secs` | integer | 30 | Max execution time |

---

## Examples

### Example 1: Create a Simple Calculator Tool

```bash
curl -X POST http://localhost:18789/api/tools \
  -H "Authorization: Bearer dev-token-123" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "calculator",
    "description": "Perform math calculations",
    "runtime": "bash",
    "body": "python3 -c \"import sys; print(eval(sys.argv[1]))\" \"$expression\"",
    "parameters": {
      "type": "object",
      "properties": {
        "expression": {
          "type": "string",
          "description": "Math expression to evaluate (e.g., 2+2, 10*5)"
        }
      },
      "required": ["expression"]
    },
    "policy": "allow",
    "sandbox": true,
    "timeout_secs": 5
  }'
```

### Example 2: Create a Slack Notifier Tool

```bash
curl -X POST http://localhost:18789/api/tools \
  -H "Authorization: Bearer dev-token-123" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "slack-notify",
    "description": "Send a message to Slack channel",
    "runtime": "bash",
    "body": "curl -X POST ${SLACK_WEBHOOK} -H \"Content-Type: application/json\" -d \"{\\\"text\\\":\\\"${message}\\\"}\"",
    "parameters": {
      "type": "object",
      "properties": {
        "message": {
          "type": "string",
          "description": "Message to send to Slack"
        }
      },
      "required": ["message"]
    },
    "policy": "elevated",
    "sandbox": false,
    "network": true,
    "timeout_secs": 10
  }'
```

### Example 3: Create a Python-Based Tool

```bash
curl -X POST http://localhost:18789/api/tools \
  -H "Authorization: Bearer dev-token-123" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "text-analysis",
    "description": "Analyze text sentiment and length",
    "runtime": "python",
    "body": "import json; text = \"${text}\"; print(json.dumps({\"length\": len(text), \"words\": len(text.split())}))",
    "parameters": {
      "type": "object",
      "properties": {
        "text": {
          "type": "string",
          "description": "Text to analyze"
        }
      },
      "required": ["text"]
    },
    "policy": "allow",
    "sandbox": true,
    "timeout_secs": 10
  }'
```

### Example 4: JavaScript to List All Tools

```javascript
async function listTools(token) {
  const response = await fetch('http://localhost:18789/api/tools', {
    headers: {
      'Authorization': `Bearer ${token}`
    }
  });

  const data = await response.json();
  console.log('Available tools:');
  data.data.tools.forEach(tool => {
    console.log(`- ${tool.name}: ${tool.description}`);
  });
}

listTools('dev-token-123');
```

### Example 5: JavaScript to Create a Tool and Test It

```javascript
async function createAndTestTool(token) {
  // Create tool
  const createResponse = await fetch('http://localhost:18789/api/tools', {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${token}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      name: 'json-formatter',
      description: 'Format and validate JSON',
      runtime: 'bash',
      body: 'python3 -m json.tool <<< "${json}"',
      parameters: {
        type: 'object',
        properties: {
          json: { type: 'string', description: 'JSON string to format' }
        },
        required: ['json']
      },
      policy: 'allow',
      timeout_secs: 5
    })
  });

  const tool = await createResponse.json();
  console.log('Tool created:', tool.data.name);

  // Test tool
  const testResponse = await fetch(
    `http://localhost:18789/api/tools/json-formatter/test`,
    {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${token}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        parameters: {
          json: '{"hello":"world","test":123}'
        }
      })
    }
  );

  const result = await testResponse.json();
  console.log('Test result:', result.data.output);
}

createAndTestTool('dev-token-123');
```

---

## Integration with LLM

### How LLM Gets Tools

```rust
// In session.rs, before sending request to LLM:

// Get all tool definitions from API
let all_tools = get_available_tools()
  .into_iter()
  .map(|tool_def| {
    // Convert to OpenAI format
    serde_json::json!({
      "type": "function",
      "function": {
        "name": tool_def.name,
        "description": tool_def.description,
        "parameters": tool_def.parameters,
      }
    })
  })
  .collect();

// Pass to LLM request
request.tools = Some(all_tools);
```

### How Tools Are Executed (Real-time)

```
User: "What's the weather in NYC?"
         ↓
LLM sees weather-check tool in available tools
         ↓
LLM calls: {"tool_name": "weather-check", "parameters": {"city": "NYC"}}
         ↓
execute_tool("weather-check", "{\"city\": \"NYC\"}")
         ↓
Tool lookup order:
   1. Built-in tools (exec, bash, etc.)
   2. Skills registry ← USER-CREATED TOOLS HERE!
   3. Plugin registry
         ↓
Tool executes, returns result
         ↓
LLM processes result and responds to user
```

---

## Implementation Steps

### Phase 1: API Routes (2-3 hours)
- [ ] Add POST `/api/tools` (create)
- [ ] Add GET `/api/tools` (list)
- [ ] Add GET `/api/tools/:name` (details)
- [ ] Add PUT `/api/tools/:name` (update)
- [ ] Add DELETE `/api/tools/:name` (delete)
- [ ] Add POST `/api/tools/:name/test` (execute test)
- [ ] Add POST `/api/tools/:name/validate` (validate)
- [ ] Add GET `/api/tools/:name/definition` (for LLM)
- [ ] Add GET `/api/tools/definitions/all` (for LLM)

### Phase 2: Persistence (1-2 hours)
- [ ] Save tools to `~/.rustyclaw/skills/user-created/`
- [ ] Load user-created tools on startup
- [ ] Create tool metadata file (`tools.json`)
- [ ] Implement tool versioning/backups

### Phase 3: Validation (1 hour)
- [ ] Validate tool name format
- [ ] Validate JSON Schema for parameters
- [ ] Check for duplicate names
- [ ] Validate bash/python syntax

### Phase 4: Integration (1 hour)
- [ ] Update `get_available_tools()` to include user tools
- [ ] Update LLM request to use all tools
- [ ] Update streaming events to show tool info

### Phase 5: Error Handling (1 hour)
- [ ] Proper error responses
- [ ] Tool execution errors
- [ ] Persistence errors
- [ ] Validation errors

---

## File Structure Changes

```
src/
├── api/
│   ├── routes.rs (ADD tool endpoints)
│   └── tools_api.rs (NEW - tool creation logic)
├── tools/
│   ├── creator.rs (NEW - tool creation/validation)
│   ├── executor.rs (UPDATE - use tool registry)
│   └── skills.rs (UPDATE - load user tools)
└── config/
    └── schema.rs (UPDATE - tool storage path)
```

---

## Benefits

✅ **Zero restart needed** - Tools available immediately
✅ **Easy to use** - Simple REST API
✅ **LLM compatible** - Automatic tool discovery
✅ **Safe** - Policy enforcement + sandboxing
✅ **Persistent** - Tools survive restarts
✅ **Testable** - Dry-run and test endpoints

---

## Testing Strategy

```bash
# Create a tool
curl POST /api/tools

# List tools (should see new tool)
curl GET /api/tools

# Ask LLM to use tool
ws://localhost:18789/ws?token=...
Message: "Use the weather-check tool to check SF"

# LLM should call tool automatically
# Response should show streaming + tool events
```

---

## Migration from Current System

Current tools stay as:
- **Built-in:** `exec`, `bash`, `send_whatsapp`, etc.
- **Skills (YAML):** Load from `~/.rustyclaw/skills/`
- **Plugins:** Via plugin registry

New tools:
- **User-created:** `/api/tools` endpoint → `~/.rustyclaw/skills/user-created/`

All coexist in same executor!

---

## Security Considerations

1. **Tool Naming:** Only alphanumeric + hyphens
2. **Parameters:** Must match JSON Schema
3. **Execution:** Still governed by policy engine
4. **Sandboxing:** Optional Docker sandbox for untrusted tools
5. **Rate limiting:** Tools can be rate-limited per session
6. **Logging:** All tool creation/execution logged

