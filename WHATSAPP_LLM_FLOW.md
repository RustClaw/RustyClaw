# WhatsApp ↔ LLM Integration Flow

## Complete Message Lifecycle

```
┌─────────────────────────────────────────────────────────────────────┐
│ 1. USER SENDS MESSAGE VIA WHATSAPP                                  │
│    "Send a WhatsApp to John: Meeting at 3pm"                        │
└────────────────┬────────────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 2. WHATSAPP ADAPTER RECEIVES EVENT                                  │
│    File: src/channels/whatsapp.rs:366-390                           │
│    - Bot receives Event::Message from WhatsApp                      │
│    - Extracts sender JID: "1234567890@s.whatsapp.net"              │
│    - Extracts text: "Send a WhatsApp to John..."                   │
└────────────────┬────────────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 3. ROUTER PROCESSES MESSAGE                                         │
│    File: src/core/router.rs:26-54                                   │
│    Call: router.handle_message(&sender, "whatsapp", &text)          │
│    - sender: "1234567890@s.whatsapp.net" (user's JID)              │
│    - channel: "whatsapp" (source channel identifier)                │
│    - content: "Send a WhatsApp to John: Meeting at 3pm"            │
└────────────────┬────────────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 4. SESSION MANAGER HANDLES CONVERSATION                             │
│    File: src/core/session.rs:82-210                                 │
│                                                                     │
│    A. Get or create session                                         │
│       - Lookup session by (user_id, channel, scope)                │
│       - If not found: create new session                            │
│                                                                     │
│    B. Add user message to history                                   │
│       - Store: {role: "user", content: "Send a WhatsApp..."}       │
│                                                                     │
│    C. Get conversation history (last 50 messages)                   │
│       - Build context window for LLM                                │
│                                                                     │
│    D. Get available tools                                           │
│       ✓ WhatsApp tools are now available:                          │
│         - send_whatsapp                                             │
│         - list_whatsapp_groups                                      │
└────────────────┬────────────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 5. LLM CHAT REQUEST WITH TOOLS                                      │
│    File: src/llm/client.rs:42-138                                   │
│                                                                     │
│    ChatRequest {                                                    │
│      model: "auto-routed",         ← Intelligent model selection   │
│      messages: [                                                    │
│        {role: "user", content: "Send a WhatsApp..."},             │
│      ],                                                             │
│      tools: [                       ← ✨ TOOLS NOW ENABLED        │
│        {                                                            │
│          name: "send_whatsapp",                                    │
│          description: "Send message to contact/group",             │
│          parameters: {...}                                         │
│        },                                                           │
│        {                                                            │
│          name: "list_whatsapp_groups",                            │
│          description: "List available WhatsApp groups",            │
│          parameters: {...}                                         │
│        }                                                            │
│      ]                                                              │
│    }                                                                │
│                                                                    │
│    LLM Response Type: TOOL_CALL                                    │
└────────────────┬────────────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 6. LLM RETURNS TOOL CALL                                            │
│    File: src/llm/client.rs:147-166                                  │
│                                                                     │
│    ChatResponse {                                                   │
│      content: "",                  ← Empty content                 │
│      tool_calls: Some([            ← ✨ TOOL CALLS                │
│        {                                                            │
│          id: "call_abc123",                                        │
│          name: "send_whatsapp",                                    │
│          arguments: "{"                                            │
│            target_type: "contact",                                 │
│            target: "John",  ← Or phone number                     │
│            message: "Meeting at 3pm"                              │
│          }"                                                         │
│        }                                                            │
│      ])                                                             │
│    }                                                                │
└────────────────┬────────────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 7. TOOL EXECUTION LOOP (In SessionManager.process_with_tools)       │
│    File: src/core/session.rs:141-210                                │
│                                                                     │
│    For each tool_call in response.tool_calls:                      │
│      A. Execute tool                                                │
│         Call: tools::execute_tool("send_whatsapp", params)         │
│                                                                     │
│      B. Route to correct tool handler                               │
│         File: src/tools/executor.rs:7-44                           │
│         match tool_name {                                           │
│           "send_whatsapp" => send_whatsapp(params).await           │
│         }                                                            │
│                                                                     │
│      C. WhatsAppService executes send                               │
│         File: src/channels/whatsapp.rs:39-62                       │
│         - Parse "John" → lookup in groups or use as phone          │
│         - Create wa::Message with text                             │
│         - Call client.send_message(jid, msg)                       │
│         - Return message_id                                         │
│                                                                     │
│      D. Add tool result to conversation history                     │
│         {role: "user", content: "Tool send_whatsapp result: ..."}  │
└────────────────┬────────────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 8. LOOP BACK TO LLM (Multi-turn Tool Conversation)                  │
│    File: src/core/session.rs:165-210                                │
│                                                                     │
│    If tool_calls were returned:                                     │
│      - Send messages + tool results back to LLM                     │
│      - LLM processes results and generates final response           │
│                                                                     │
│    New ChatRequest {                                                │
│      messages: [                                                    │
│        {role: "user", content: "Send a WhatsApp to John..."},     │
│        {role: "assistant", content: "I'll send that message"},    │
│        {role: "user", content: "Tool send_whatsapp result: ✓..."}  │
│      ],                                                             │
│      tools: [...same tools...]     ← Available for more calls      │
│    }                                                                │
│                                                                     │
│    LLM Response Type: TEXT (no more tool calls)                    │
│    Content: "✓ I've sent the message to John about the meeting"   │
└────────────────┬────────────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 9. FINAL RESPONSE STORED & RETURNED                                 │
│    File: src/core/session.rs:200-210                                │
│                                                                     │
│    A. Store assistant response in history                           │
│       {role: "assistant", content: "✓ I've sent the message..."}  │
│                                                                     │
│    B. Return MessageResponse to Router                              │
│       content: "✓ I've sent the message to John..."               │
│       model: "qwen2.5:7b"                                          │
│       tokens: 45                                                    │
└────────────────┬────────────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 10. SEND RESPONSE BACK TO WHATSAPP USER                             │
│     File: src/channels/whatsapp.rs:390-406                          │
│                                                                     │
│     Create wa::Message with response content:                      │
│     wa::Message {                                                   │
│       conversation: Some("✓ I've sent the message to John...")    │
│     }                                                                │
│                                                                     │
│     Send via ctx.send_message(reply)                               │
│     - Uses MessageContext from event                                │
│     - Sends reply to same conversation                              │
│     - Works for 1-on-1 and group chats                             │
└────────────────┬────────────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 11. USER SEES RESPONSE                                              │
│     WhatsApp: "✓ I've sent the message to John..."                │
│                                                                     │
│     PLUS: John also received the WhatsApp:                         │
│     "Meeting at 3pm"                                                │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Key Data Flow Points

### User ID & Channel Tracking
```rust
// WhatsApp sender JID maps to RustyClaw user_id
sender: "1234567890@s.whatsapp.net"
↓
// Router extracts just the number for session scoping
router.handle_message("1234567890@s.whatsapp.net", "whatsapp", text)
↓
// SessionManager creates session key
session_key: (user_id="1234567890@s.whatsapp.net", channel="whatsapp", scope="per-sender")
```

### Tool Availability
```rust
// In session.rs::get_available_tools()
fn get_available_tools(&self) -> Vec<ToolDefinition> {
    if crate::get_whatsapp_service().is_some() {
        // ✓ WhatsApp service is available
        crate::tools::whatsapp::get_whatsapp_tool_definitions()
    } else {
        Vec::new()  // No tools if service unavailable
    }
}
```

**Tools Available:**
1. `send_whatsapp` - Send to contact or group
2. `list_whatsapp_groups` - List all groups

### Tool Execution Parameters
```json
{
  "send_whatsapp": {
    "target_type": "contact|group",
    "target": "phone_number|group_name|group_id",
    "message": "Text to send"
  }
}
```

**Target Resolution:**
- `target_type: "contact"` + `target: "1234567890"` → Phone number
- `target_type: "group"` + `target: "Team Alpha"` → Group name (auto-lookup)
- `target_type: "group"` + `target: "123456789@g.us"` → Direct JID

---

## Session Memory & Context

### Conversation History
Each WhatsApp user gets their own session with full conversation history:

```
Session ID: uuid-abc123
User: 1234567890@s.whatsapp.net (WhatsApp sender)
Channel: whatsapp
Created: 2024-02-03T12:00:00Z

Messages:
1. {role: "user", content: "Send a WhatsApp to John: Meeting at 3pm"}
2. {role: "assistant", content: "(tool call)"}
3. {role: "user", content: "Tool send_whatsapp result: ✓ Message sent"}
4. {role: "assistant", content: "✓ I've sent the message to John"}
```

**Context Window:** Last 50 messages (configurable)
**Memory:** Full conversation history persisted in SQLite
**Scope:** Per-sender (each WhatsApp sender has separate session)

---

## Multi-Turn Conversation Example

```
User: "Send WhatsApp to John saying meeting moved to 4pm"
  ↓
LLM: "I'll send that message to John"
  ↓
Tool Call: send_whatsapp(contact, "John", "Meeting moved to 4pm")
  ↓
Tool Result: "✓ Message sent (ID: msg_123)"
  ↓
LLM: "Done! I've sent the update to John about the new meeting time"
  ↓
WhatsApp User: "Done! I've sent the update to John..."

---

User (in same conversation): "What groups do I have?"
  ↓
LLM: "I'll check your WhatsApp groups"
  ↓
Tool Call: list_whatsapp_groups()
  ↓
Tool Result: "Groups: Team Alpha (5 members), Project X (3 members)..."
  ↓
LLM: "You have 2 WhatsApp groups: Team Alpha and Project X"
  ↓
WhatsApp User: "You have 2 WhatsApp groups: Team Alpha and Project X"
```

---

## Error Handling & Recovery

### If Tool Fails
```
Tool Execution Error: "Group 'John' not found"
  ↓
Error added to history as tool result
  ↓
LLM processes error and responds appropriately
  ↓
User sees: "I couldn't find a group named 'John'. Did you mean a contact?"
```

### If WhatsApp Service Unavailable
```
No tools available in get_available_tools()
  ↓
ChatRequest sent without tools
  ↓
LLM responds normally without tool calls
  ↓
User gets: "I can't send WhatsApp messages right now"
```

---

## Current Status

✅ **Complete End-to-End Flow**
- ✅ WhatsApp messages received and parsed
- ✅ Router creates sessions and tracks users
- ✅ SessionManager fetches available tools
- ✅ LLM receives tool definitions
- ✅ LLM can decide to call tools
- ✅ Tool execution with proper error handling
- ✅ Multi-turn conversation loop
- ✅ Final response sent back to WhatsApp
- ✅ Full conversation history maintained

---

## What Happens Next with CI/CD

When you push to GitHub:
1. Tests verify all components work together
2. Code is linted and formatted
3. Binary is built for Proxmox deployment
4. Live instance updated (if self-hosted runner active)

**You can now:**
```
1. Connect WhatsApp via QR code
2. Send messages to WhatsApp
3. Let LLM decide when/how to respond
4. LLM can send WhatsApp messages using tools
5. Have rich conversations with memory
```

---

## Next Steps for Enhanced UX

### Quick Wins (30 mins)
- [ ] Add message delivery confirmation
- [ ] Show group member count in listing
- [ ] Format group list with emoji

### Medium Effort (2-3 hours)
- [ ] Add media support (images, documents)
- [ ] Implement message editing/deletion
- [ ] Add typing indicators

### Advanced (4-6 hours)
- [ ] Scheduled messages via tools
- [ ] Group management (add/remove members)
- [ ] Contact name resolution
- [ ] Message reactions/replies

