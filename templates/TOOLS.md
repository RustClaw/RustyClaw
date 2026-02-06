# Tool Usage Guide

## Available Tools

You have access to these tools - use them proactively:
- `exec` / `bash`: Execute shell commands
- `web_fetch`: Fetch content from URLs
- `web_search`: Search the web
- `create_tool`: Create new persistent tools/capabilities
- `delete_tool`: Remove tools you created

## Creating New Tools

When you need a capability that doesn't exist, use `create_tool` to create it.
DO NOT output code as text - call the function directly!

### create_tool Schema

```json
{
  "name": "string (required) - Unique tool name, alphanumeric with underscores/hyphens",
  "description": "string (required) - Clear description of what the tool does",
  "runtime": "string (required) - Either 'bash' or 'python'",
  "body": "string (required) - The executable script content",
  "parameters": {
    "type": "object",
    "properties": {
      "param_name": {"type": "string", "description": "Parameter description"}
    },
    "required": ["param_name"]
  },
  "policy": "string (optional) - 'allow' (default) or 'elevated' for dangerous tools",
  "sandbox": "boolean (optional) - Whether to run in Docker sandbox (default: true)"
}
```

### Example: Weather Tool Creation

To create a weather tool, call `create_tool` with:

```json
{
  "name": "get_weather",
  "description": "Fetches current weather for a city using wttr.in",
  "runtime": "bash",
  "body": "curl -s \"wttr.in/${city}?format=%C+%t+%h+%w\"",
  "parameters": {
    "type": "object",
    "properties": {
      "city": {"type": "string", "description": "City name to get weather for"}
    },
    "required": ["city"]
  },
  "policy": "allow"
}
```

### Tool Creation Workflow

1. Call `create_tool` with the JSON above - the tool is created immediately
2. Call your new tool right away (e.g., call `get_weather` with `{"city": "São Paulo"}`)
3. If the tool fails, analyze the error, call `create_tool` again with fixes, and retry
4. You can retry up to 10 times - be persistent!
