# RustyClaw API Documentation

**Version:** 1.0.0
**Status:** Production Ready
**Last Updated:** 2026-02-03

---

## Table of Contents
1. [Overview](#overview)
2. [Authentication](#authentication)
3. [Base URL & Configuration](#base-url--configuration)
4. [Error Handling](#error-handling)
5. [HTTP Endpoints](#http-endpoints)
6. [WebSocket Connection](#websocket-connection)
7. [Event Streaming (SSE)](#event-streaming-sse)
8. [Data Models](#data-models)
9. [Examples](#examples)
10. [Rate Limiting & Best Practices](#rate-limiting--best-practices)

---

## Overview

RustyClaw is a **local-first, privacy-focused AI assistant gateway** that connects to locally-running LLMs (Ollama, llama.cpp, vLLM). It provides:

- ✅ **Real-time streaming responses** via WebSocket and HTTP SSE
- ✅ **Persistent conversation history** stored in SQLite
- ✅ **Tool/function execution** during conversations
- ✅ **Session management** with configurable scoping
- ✅ **Token tracking** for analytics
- ✅ **Multi-channel support** (Telegram, Discord, WhatsApp, Web)

### Key Features for Frontend Development

| Feature | Availability | Details |
|---------|--------------|---------|
| **Streaming Chat** | WebSocket + SSE | Real-time token-by-token responses |
| **Session Management** | REST API | Create, list, get, delete sessions |
| **History Retrieval** | REST API | Get full conversation history |
| **Tool Execution** | Streaming events | See tool_start, tool_end events |
| **Authentication** | Token-based | Bearer token or query parameter |
| **Error Handling** | Structured JSON | Detailed error codes and messages |

---

## Authentication

All API requests require a **bearer token** (for HTTP) or **query parameter** (for WebSocket).

### Token Format
- **Source:** Configured in `config.yaml` under `api.tokens`
- **Can be:** Any string (e.g., `"api-key-prod-123"` or `"web-user-alice"`)
- **User ID extraction:** Tokens with prefix `web-user-` → user ID is suffix
  - Example: `web-user-alice` → user_id = `alice`
  - Example: `custom-token-123` → user_id = `custom-token-123`

### HTTP Authentication
```bash
Authorization: Bearer YOUR_TOKEN_HERE
```

### WebSocket Authentication
```javascript
ws://localhost:18789/ws?token=YOUR_TOKEN_HERE
```

### cURL Examples
```bash
# HTTP Request
curl -H "Authorization: Bearer your-token" \
     http://localhost:18789/api/chat \
     -d '{"message":"Hello"}'

# WebSocket (with tools like websocat)
websocat "ws://localhost:18789/ws?token=your-token"
```

---

## Base URL & Configuration

### Default Configuration
- **Host:** `127.0.0.1` (localhost only)
- **Port:** `18789`
- **Enabled:** Add to `config.yaml`:
  ```yaml
  api:
    enabled: true
    host: "0.0.0.0"  # Change to accept external connections
    port: 18789
    tokens:
      - "dev-token-123"
      - "web-user-alice"
      - "${API_TOKEN}"  # Environment variable substitution
  ```

### URLs
- **HTTP API:** `http://localhost:18789/api`
- **WebSocket:** `ws://localhost:18789/ws`
- **Health Check:** `http://localhost:18789/health`

---

## Error Handling

### Error Response Format
```json
{
  "status": "error",
  "code": "ERROR_CODE",
  "message": "Human-readable error description",
  "timestamp": "2026-02-03T12:34:56Z"
}
```

### HTTP Status Codes
| Code | Meaning | When |
|------|---------|------|
| `200` | Success | Request completed successfully |
| `201` | Created | Session/resource created |
| `204` | No Content | Deletion successful |
| `400` | Bad Request | Invalid input (empty message, too long, etc.) |
| `401` | Unauthorized | Missing or invalid token |
| `403` | Forbidden | Access denied |
| `500` | Server Error | Internal error, check logs |
| `503` | Service Unavailable | LLM service not responding |

### Common Error Codes
- `UNAUTHORIZED` - Missing or invalid token
- `BAD_REQUEST` - Validation failed
- `NOT_FOUND` - Session/message not found
- `SERVICE_UNAVAILABLE` - LLM backend offline
- `INTERNAL_ERROR` - Unexpected server error

---

## HTTP Endpoints

### 1. Health Check
**Check API and LLM status**

```
GET /health
```

**Response:**
```json
{
  "status": "ok",
  "version": "0.1.0",
  "gateway": "rustyclaw"
}
```

---

### 2. Chat (Non-Streaming)
**Send message and get complete response**

```
POST /api/chat
Authorization: Bearer YOUR_TOKEN
Content-Type: application/json
```

**Request Body:**
```json
{
  "message": "What is the capital of France?",
  "stream": false,
  "session_id": "optional-session-id"
}
```

**Response:**
```json
{
  "status": "success",
  "data": {
    "status": "success",
    "message_id": "msg-550e8400-e29b-41d4-a716-446655440000",
    "session_id": "sess-550e8400-e29b-41d4-a716-446655440001",
    "user_id": "alice",
    "timestamp": "2026-02-03T12:34:56Z",
    "input": {
      "text": "What is the capital of France?",
      "tokens": 8,
      "model": null
    },
    "response": {
      "text": "The capital of France is Paris.",
      "tokens": 12,
      "model": "qwen2.5:7b"
    },
    "latency_ms": 1250
  }
}
```

**Parameters:**
- `message` *(required, string)* - User message, max 10,000 characters
- `stream` *(optional, boolean)* - Default: `false`. Set to `true` for SSE streaming
- `session_id` *(optional, string)* - Reuse existing session (optional, auto-created)

---

### 3. Chat (Streaming via SSE)
**Get streaming response via Server-Sent Events**

```
POST /api/chat
Authorization: Bearer YOUR_TOKEN
Content-Type: application/json
```

**Request Body:**
```json
{
  "message": "Write a poem about AI",
  "stream": true
}
```

**Response:** Event stream (see [Event Streaming](#event-streaming-sse))

---

### 4. Create Session
**Explicitly create a new session**

```
POST /api/sessions
Authorization: Bearer YOUR_TOKEN
Content-Type: application/json
```

**Request Body:**
```json
{
  "scope": "per-sender"
}
```

**Response:**
```json
{
  "status": "success",
  "data": {
    "id": "sess-550e8400-e29b-41d4-a716-446655440001",
    "user_id": "alice",
    "channel": "web",
    "scope": "per-sender",
    "created_at": "2026-02-03T12:34:56Z",
    "updated_at": "2026-02-03T12:34:56Z",
    "message_count": 0,
    "tokens_used": 0,
    "context_window": 128000,
    "status": "active"
  }
}
```

---

### 5. List Sessions
**Get all sessions for current user**

```
GET /api/sessions
Authorization: Bearer YOUR_TOKEN
```

**Response:**
```json
{
  "status": "success",
  "data": {
    "sessions": [
      {
        "id": "sess-550e8400-e29b-41d4-a716-446655440001",
        "user_id": "alice",
        "channel": "web",
        "scope": "per-sender",
        "created_at": "2026-02-03T12:34:56Z",
        "updated_at": "2026-02-03T12:35:00Z",
        "message_count": 5,
        "tokens_used": 1250,
        "context_window": 128000,
        "status": "active"
      }
    ],
    "total": 1,
    "limit": 100,
    "offset": 0
  }
}
```

---

### 6. Get Session Details
**Get specific session info**

```
GET /api/sessions/:id
Authorization: Bearer YOUR_TOKEN
```

**Response:** Same as individual session in list

---

### 7. Delete Session (Clear History)
**Clear all messages in a session**

```
DELETE /api/sessions/:id
Authorization: Bearer YOUR_TOKEN
```

**Response:**
```
204 No Content
```

---

### 8. Get Messages (History)
**Retrieve conversation history**

```
GET /api/messages?limit=50&offset=0
Authorization: Bearer YOUR_TOKEN
```

**Query Parameters:**
- `limit` *(optional, integer)* - Max 500, default 50
- `offset` *(optional, integer)* - Pagination offset, default 0

**Response:**
```json
{
  "status": "success",
  "data": {
    "session_id": "sess-550e8400-e29b-41d4-a716-446655440001",
    "messages": [
      {
        "id": "msg-550e8400-e29b-41d4-a716-446655440000",
        "session_id": "sess-550e8400-e29b-41d4-a716-446655440001",
        "user_id": "alice",
        "channel": "web",
        "role": "user",
        "content": "What is 2+2?",
        "timestamp": "2026-02-03T12:34:56Z",
        "tokens": 5,
        "model_used": null
      },
      {
        "id": "msg-550e8400-e29b-41d4-a716-446655440001",
        "session_id": "sess-550e8400-e29b-41d4-a716-446655440001",
        "user_id": "alice",
        "channel": "web",
        "role": "assistant",
        "content": "2 + 2 = 4",
        "timestamp": "2026-02-03T12:34:57Z",
        "tokens": 8,
        "model_used": "qwen2.5:7b"
      }
    ],
    "total": 2,
    "limit": 50,
    "offset": 0
  }
}
```

---

### 9. Get Single Message
**Get details of a specific message**

```
GET /api/messages/:id
Authorization: Bearer YOUR_TOKEN
```

**Response:** Single message object (from history)

---

### 10. List Available Models
**Get loaded LLM models**

```
GET /api/models
Authorization: Bearer YOUR_TOKEN
```

**Response:**
```json
{
  "status": "success",
  "data": {
    "models": [
      {
        "name": "qwen2.5:7b",
        "role": "primary",
        "vram_mb": 4096,
        "loaded": true
      },
      {
        "name": "mistral:7b",
        "role": "code",
        "vram_mb": 4096,
        "loaded": false
      }
    ]
  }
}
```

---

### 11. Load Model
**Load a model into memory**

```
POST /api/models/:name/load
Authorization: Bearer YOUR_TOKEN
```

**Response:**
```json
{
  "status": "success",
  "data": {
    "name": "qwen2.5:7b",
    "loaded": true,
    "vram_mb": 4096
  }
}
```

---

## WebSocket Connection

### Connection
```javascript
const socket = new WebSocket('ws://localhost:18789/ws?token=YOUR_TOKEN');
```

### Message Flow

#### 1. Client Connects
Server responds with:
```json
{
  "type": "connected",
  "session_id": "sess-550e8400-e29b-41d4-a716-446655440001"
}
```

#### 2. Client Sends Message
```json
{
  "type": "message",
  "content": "Tell me a joke"
}
```

#### 3. Server Starts Response
```json
{
  "type": "start",
  "session_id": "sess-550e8400-e29b-41d4-a716-446655440001",
  "message_id": "msg-550e8400-e29b-41d4-a716-446655440000"
}
```

#### 4. Server Streams Content (One event per token or token chunk)
```json
{
  "type": "stream",
  "content": "Why"
}
```
```json
{
  "type": "stream",
  "content": " did"
}
```
```json
{
  "type": "stream",
  "content": " the"
}
```

#### 5. Tool Execution Events (if tools are used)
```json
{
  "type": "tool_use",
  "name": "bash",
  "status": "running"
}
```
```json
{
  "type": "stream",
  "content": " execute "
}
```
```json
{
  "type": "tool_use",
  "name": "bash",
  "status": "done"
}
```

#### 6. Server Completes Response
```json
{
  "type": "end",
  "message_id": "msg-550e8400-e29b-41d4-a716-446655440000",
  "total_tokens": 42,
  "model": "qwen2.5:7b",
  "latency_ms": 1234
}
```

#### 7. Error (if any)
```json
{
  "type": "error",
  "error": "LLM service unavailable",
  "error_code": 503
}
```

#### 8. Keepalive
Server sends every 30 seconds:
```json
{
  "type": "ping"
}
```

Client responds:
```json
{
  "type": "pong"
}
```

---

## Event Streaming (SSE)

### Connection
```bash
curl -H "Authorization: Bearer YOUR_TOKEN" \
     -X POST \
     -H "Content-Type: application/json" \
     -d '{"message":"Hello","stream":true}' \
     http://localhost:18789/api/chat
```

### Response Format
Server sends events in this format:
```
event: (default or specific event type)
data: (content)

event: (next event)
data: (content)
```

### Event Types

**1. Content Delta (one per token/chunk)**
```
event:
data: Hello
```

**2. Tool Start**
```
event: tool_start
data: bash
```

**3. Tool End**
```
event: tool_end
data: {"name":"bash","result":"output here"}
```

**4. Completion**
```
event: done
data: {"model":"qwen2.5:7b","usage":{"prompt_tokens":10,"completion_tokens":32,"total_tokens":42}}
```

**5. Error**
```
event: error
data: LLM service error
```

### Client-Side Handling (JavaScript)
```javascript
const eventSource = new EventSource(
  'http://localhost:18789/api/chat?stream=true',
  {
    headers: {
      'Authorization': 'Bearer YOUR_TOKEN',
      'Content-Type': 'application/json'
    }
  }
);

eventSource.addEventListener('message', (event) => {
  // Default content delta events
  console.log('Content:', event.data);
  displayContent(event.data);
});

eventSource.addEventListener('tool_start', (event) => {
  console.log('Tool executing:', event.data);
});

eventSource.addEventListener('tool_end', (event) => {
  const result = JSON.parse(event.data);
  console.log('Tool result:', result);
});

eventSource.addEventListener('done', (event) => {
  const completion = JSON.parse(event.data);
  console.log('Complete! Model:', completion.model);
  console.log('Total tokens:', completion.usage.total_tokens);
  eventSource.close();
});

eventSource.addEventListener('error', (event) => {
  console.error('Error:', event.data);
  eventSource.close();
});
```

---

## Data Models

### Session
```json
{
  "id": "sess-550e8400-e29b-41d4-a716-446655440001",
  "user_id": "alice",
  "channel": "web",
  "scope": "per-sender",
  "created_at": "2026-02-03T12:34:56Z",
  "updated_at": "2026-02-03T12:35:00Z",
  "message_count": 5,
  "tokens_used": 1250,
  "context_window": 128000,
  "status": "active"
}
```

### Message
```json
{
  "id": "msg-550e8400-e29b-41d4-a716-446655440000",
  "session_id": "sess-550e8400-e29b-41d4-a716-446655440001",
  "user_id": "alice",
  "channel": "web",
  "role": "user|assistant|tool",
  "content": "Message text or response",
  "timestamp": "2026-02-03T12:34:56Z",
  "tokens": 42,
  "model_used": "qwen2.5:7b"
}
```

### Chat Request
```json
{
  "message": "Your question here",
  "stream": false,
  "session_id": "optional-session-id"
}
```

### Chat Response (Non-streaming)
```json
{
  "status": "success",
  "message_id": "msg-xxx",
  "session_id": "sess-xxx",
  "user_id": "alice",
  "timestamp": "2026-02-03T12:34:56Z",
  "input": {
    "text": "Your question",
    "tokens": 8,
    "model": null
  },
  "response": {
    "text": "Answer text",
    "tokens": 24,
    "model": "qwen2.5:7b"
  },
  "latency_ms": 1234
}
```

---

## Examples

### Example 1: Simple Chat (cURL)
```bash
curl -X POST http://localhost:18789/api/chat \
  -H "Authorization: Bearer dev-token-123" \
  -H "Content-Type: application/json" \
  -d '{"message":"What is 2+2?"}'
```

### Example 2: Streaming Chat (JavaScript/Node)
```javascript
async function streamChat(message, token) {
  const response = await fetch('http://localhost:18789/api/chat', {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${token}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      message: message,
      stream: true
    })
  });

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split('\n');
    buffer = lines.pop(); // Keep incomplete line in buffer

    for (const line of lines) {
      if (line.startsWith('data: ')) {
        const data = line.slice(6);
        console.log('Content:', data);
      }
    }
  }
}

streamChat('Tell me a story', 'dev-token-123');
```

### Example 3: WebSocket Chat (JavaScript)
```javascript
const socket = new WebSocket('ws://localhost:18789/ws?token=dev-token-123');
let sessionId = null;
let currentContent = '';

socket.addEventListener('open', () => {
  console.log('Connected');
});

socket.addEventListener('message', (event) => {
  const msg = JSON.parse(event.data);

  switch (msg.type) {
    case 'connected':
      sessionId = msg.session_id;
      console.log('Session:', sessionId);

      // Send a message
      socket.send(JSON.stringify({
        type: 'message',
        content: 'Hello AI!'
      }));
      break;

    case 'start':
      currentContent = '';
      console.log('Message started:', msg.message_id);
      break;

    case 'stream':
      currentContent += msg.content;
      console.log('Streaming:', msg.content);
      break;

    case 'tool_use':
      if (msg.status === 'running') {
        console.log('Tool executing:', msg.name);
      } else {
        console.log('Tool done:', msg.name);
      }
      break;

    case 'end':
      console.log('Complete!');
      console.log('Full response:', currentContent);
      console.log('Tokens:', msg.total_tokens);
      console.log('Model:', msg.model);
      console.log('Latency:', msg.latency_ms + 'ms');
      break;

    case 'error':
      console.error('Error:', msg.error);
      break;

    case 'ping':
      socket.send(JSON.stringify({ type: 'pong' }));
      break;
  }
});

socket.addEventListener('close', () => {
  console.log('Disconnected');
});
```

### Example 4: Session Management (Python)
```python
import requests
import json

BASE_URL = "http://localhost:18789"
TOKEN = "dev-token-123"
HEADERS = {"Authorization": f"Bearer {TOKEN}"}

# List sessions
sessions = requests.get(f"{BASE_URL}/api/sessions", headers=HEADERS).json()
print("Sessions:", sessions)

# Get messages from session
session_id = sessions['data']['sessions'][0]['id']
messages = requests.get(
    f"{BASE_URL}/api/messages?limit=10",
    headers=HEADERS
).json()
print("Messages:", messages)

# Send a chat message
response = requests.post(
    f"{BASE_URL}/api/chat",
    headers=HEADERS,
    json={"message": "What models are available?"}
).json()
print("Response:", response['data']['response'])

# Get available models
models = requests.get(f"{BASE_URL}/api/models", headers=HEADERS).json()
print("Models:", models)

# Clear history
requests.delete(f"{BASE_URL}/api/sessions/{session_id}", headers=HEADERS)
print("Session cleared")
```

---

## Rate Limiting & Best Practices

### Best Practices

1. **Reuse Sessions**
   - Don't create new session for each message
   - Sessions maintain conversation history
   - Reuse `session_id` when possible

2. **Handle Streaming Properly**
   - Buffer streams if needed
   - Handle connection drops gracefully
   - Implement reconnection logic

3. **Token Management**
   - Don't expose tokens in client code
   - Use environment variables or secure storage
   - Rotate tokens periodically

4. **Error Handling**
   - Always handle HTTP error codes
   - Implement exponential backoff for retries
   - Log errors for debugging

5. **Performance**
   - Use streaming for long responses
   - Batch non-critical requests
   - Cache session IDs and model info

6. **Resource Usage**
   - Monitor token counts
   - Set message length limits
   - Implement request timeouts

### Suggested Timeouts
- **HTTP requests:** 30 seconds
- **WebSocket connection:** 5 seconds
- **Streaming events:** 60 seconds per event

### Request Examples with Timeouts

**JavaScript:**
```javascript
const controller = new AbortController();
const timeout = setTimeout(() => controller.abort(), 30000);

fetch(url, { signal: controller.signal })
  .finally(() => clearTimeout(timeout));
```

**Python:**
```python
requests.get(url, timeout=30)
```

**cURL:**
```bash
curl --connect-timeout 5 --max-time 30 http://localhost:18789/api/health
```

---

## Configuration Example

**`config.yaml`:**
```yaml
gateway:
  host: "127.0.0.1"
  port: 7860
  log_level: "info"

llm:
  provider: "ollama"
  base_url: "http://localhost:11434/v1"
  models:
    primary: "qwen2.5:7b"
    code: "mistral:7b"
    fast: "qwen2.5:1.5b"

api:
  enabled: true
  host: "0.0.0.0"
  port: 18789
  tokens:
    - "dev-token-123"
    - "web-user-alice"
    - "${API_TOKEN}"

sessions:
  scope: "per-sender"
  max_tokens: 128000
```

---

## Support & Debugging

### Check API Health
```bash
curl http://localhost:18789/health
```

### View Logs
```bash
# Docker
docker-compose logs -f rustyclaw

# Local
RUST_LOG=debug cargo run -- serve
```

### Test Token
```bash
curl -H "Authorization: Bearer YOUR_TOKEN" \
     http://localhost:18789/api/sessions
```

### Common Issues

| Issue | Solution |
|-------|----------|
| 401 Unauthorized | Check token in config, verify Authorization header |
| 503 Service Unavailable | Ensure LLM (Ollama) is running on correct port |
| Connection timeout | Check firewall, verify API is enabled in config |
| High latency | Reduce model size or increase LLM resources |

---

## SDK Recommendations

### Web/JavaScript
- Use native `fetch` API or `axios`
- For WebSocket: native `WebSocket` or `socket.io`
- For SSE: native `EventSource`

### Android
- Use `okhttp3` or `retrofit` for HTTP
- Use `OkHttp WebSocketListener` for WebSocket
- Handle configuration changes and lifecycle

### iOS
- Use `URLSession` for HTTP
- Use `URLSessionWebSocketTask` for WebSocket
- Use `URLSessionStreamTask` for SSE
- Handle background modes

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-02-03 | Initial release with streaming support |

---

**Generated:** 2026-02-03
**For issues:** Check RustyClaw repository on GitHub
**License:** MIT
