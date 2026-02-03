# WhatsApp ↔ LLM Code Flow - Technical Reference

## Entry Point: WhatsApp Message Received

**File:** `src/channels/whatsapp.rs:370-425`

```rust
// WhatsApp event fires
Event::Message(message, info) => {
    // STEP 1: Extract message details
    let sender = info.source.sender.to_string();  // "1234567890@s.whatsapp.net"
    let text = message.conversation.clone();      // "Send WhatsApp to John..."

    // STEP 2: Route to handler
    match router.handle_message(&sender, "whatsapp", &text).await {
        Ok(response) => {
            // STEP 3: Send response back to WhatsApp
            let reply = wa::Message {
                conversation: Some(response.content),  // "✓ Message sent"
                ..Default::default()
            };
            ctx.send_message(reply).await?;
        }
    }
}
```

---

## Router: Direction Traffic

**File:** `src/core/router.rs:26-54`

```rust
pub async fn handle_message(
    &self,
    user_id: &str,                    // "1234567890@s.whatsapp.net"
    channel: &str,                    // "whatsapp"
    content: &str,                    // "Send WhatsApp to John..."
) -> Result<MessageResponse> {
    // STEP 1: Get/create user session
    let session = self
        .session_manager
        .get_or_create_session(user_id, channel)
        .await?;

    // STEP 2: Process message (handles LLM + tools)
    let response = self
        .session_manager
        .process_message(&session.id, content)
        .await?;

    // STEP 3: Return response
    Ok(response)  // MessageResponse { content, model, tokens }
}
```

---

## SessionManager: LLM + Tools Pipeline

**File:** `src/core/session.rs:82-210`

### Part A: Message Processing Entry
```rust
pub async fn process_message(
    &self,
    session_id: &str,
    user_message: &str,
) -> Result<MessageResponse> {
    // STEP 1: Store user message in history
    self.add_message(session_id, "user", user_message, None, None).await?;

    // STEP 2: Start LLM conversation with tools
    self.process_with_tools(session_id, tools).await
}
```

### Part B: Tool-Enabled LLM Loop
```rust
async fn process_with_tools(
    &self,
    session_id: &str,
    tools: Vec<ToolDefinition>,
) -> Result<MessageResponse> {
    // STEP 1: Get conversation history
    let history = self.storage.get_messages(session_id, Some(50)).await?;
    let mut llm_messages: Vec<ChatMessage> = history.iter()
        .map(|msg| ChatMessage {
            role: msg.role.clone(),
            content: msg.content.clone(),
        })
        .collect();

    // STEP 2: Determine model
    let model = self.llm_client.route_model(last_user_msg).to_string();

    // STEP 3: TOOL CALLING LOOP
    loop {
        // STEP 3A: Send to LLM WITH TOOLS
        let request = ChatRequest {
            model: model.clone(),
            messages: llm_messages.clone(),
            tools: Some(tools.clone()),  // ← TOOLS ENABLED HERE
        };

        let response = self.llm_client.chat(request).await?;

        // STEP 3B: Check if LLM wants to call tools
        if let Some(tool_calls) = response.tool_calls {
            // STEP 3C: Add assistant message (tool use)
            llm_messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: response.content.clone(),
            });

            // STEP 3D: Execute each tool call
            for tool_call in tool_calls {
                let result = crate::tools::execute_tool(
                    &tool_call.name,        // "send_whatsapp"
                    &tool_call.arguments    // JSON: {target_type, target, message}
                ).await;

                // STEP 3E: Add tool result to history
                llm_messages.push(ChatMessage {
                    role: "user".to_string(),
                    content: format!("Tool {} result: {:?}", tool_call.name, result),
                });
            }

            // LOOP BACK to STEP 3A with tool results
        } else {
            // STEP 4: No more tools - final response
            self.add_message(
                session_id,
                "assistant",
                &response.content,
                Some(&response.model),
                response.usage.map(|u| u.total_tokens),
            ).await?;

            return Ok(MessageResponse {
                content: response.content,
                model: response.model,
                tokens: response.usage.map(|u| u.total_tokens),
            });
        }
    }
}
```

---

## Tool Selection: What Tools Are Available?

**File:** `src/core/session.rs:227-237`

```rust
fn get_available_tools(&self) -> Vec<ToolDefinition> {
    // Check if WhatsApp service is running
    if crate::get_whatsapp_service().is_some() {
        // ✓ Return WhatsApp tools
        crate::tools::whatsapp::get_whatsapp_tool_definitions()
    } else {
        // ✗ Service unavailable - no tools
        Vec::new()
    }
}
```

**Returned Tools:**
```rust
// From src/tools/whatsapp.rs:81-157
vec![
    ToolDefinition {
        name: "send_whatsapp",
        description: "Send a WhatsApp message to a contact or group",
        parameters: {
            "type": "object",
            "properties": {
                "target_type": {"type": "string", "enum": ["contact", "group"]},
                "target": {"type": "string"},
                "message": {"type": "string"}
            },
            "required": ["target_type", "target", "message"]
        }
    },
    ToolDefinition {
        name: "list_whatsapp_groups",
        description: "List all available WhatsApp groups",
        parameters: {"type": "object", "properties": {}}
    }
]
```

---

## LLM: Chat with Tools

**File:** `src/llm/client.rs:42-138`

```rust
pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
    // STEP 1: Determine model to use
    let model = if request.model.is_empty() {
        self.router.route(last_message).to_string()
    } else {
        request.model.clone()
    };

    // STEP 2: Build OpenAI-compatible request
    let mut req_builder = CreateChatCompletionRequestArgs::default();
    req_builder.model(&model);
    req_builder.messages(messages);

    // STEP 3: ADD TOOLS TO REQUEST
    if let Some(tools) = request.tools {
        let converted_tools: Vec<ChatCompletionTool> = tools
            .into_iter()
            .filter_map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
                // Convert to ChatCompletionTool...
            })
            .collect();

        req_builder.tools(converted_tools);
        req_builder.tool_choice(ChatCompletionToolChoiceOption::Auto);
    }

    // STEP 4: Send to LLM (Ollama, vLLM, etc.)
    let response = self.client.chat().create(req).await?;

    // STEP 5: Extract tool calls if present
    let tool_calls = choice.message.tool_calls.as_ref().map(|calls| {
        calls
            .iter()
            .map(|call| {
                match call {
                    ChatCompletionMessageToolCall::Function { id, function } => {
                        ToolCall {
                            id: id.clone(),
                            name: function.name.clone(),
                            arguments: function.arguments.clone(),
                        }
                    }
                }
            })
            .collect()
    });

    // STEP 6: Return response with tool calls
    Ok(ChatResponse {
        content: choice.message.content.clone().unwrap_or_default(),
        model: response.model,
        tool_calls,  // ← May be Some([...]) or None
        // ...
    })
}
```

---

## Tool Execution: Route to Implementation

**File:** `src/tools/executor.rs:7-44`

```rust
pub async fn execute_tool(name: &str, arguments: &str) -> Result<String> {
    info!("Executing tool: {} with arguments: {}", name, arguments);

    // STEP 1: Parse arguments from JSON string
    match name {
        "send_whatsapp" => {
            // Parse parameters
            let params: SendWhatsAppParams = serde_json::from_str(arguments)?;

            // Execute the tool
            send_whatsapp(params).await
        }
        "list_whatsapp_groups" => {
            let params: ListWhatsAppGroupsParams = serde_json::from_str(arguments)?;
            list_whatsapp_groups(params).await
        }
        _ => Err(anyhow!("Unknown tool: {}", name)),
    }
}
```

---

## WhatsApp Tool: Send Message

**File:** `src/tools/whatsapp.rs:20-40`

```rust
pub async fn send_whatsapp(params: SendWhatsAppParams) -> Result<String> {
    // STEP 1: Get the WhatsApp service
    let service = crate::get_whatsapp_service()
        .context("WhatsApp service not available")?;

    // STEP 2: Route based on target type
    let message_id = match params.target_type.as_str() {
        "contact" => {
            service.send_to_contact(&params.target, &params.message).await?
        }
        "group" => {
            service.send_to_group(&params.target, &params.message).await?
        }
        _ => anyhow::bail!("Invalid target_type"),
    };

    // STEP 3: Return result
    Ok(format!("✓ WhatsApp message sent successfully (ID: {})", message_id))
}
```

---

## WhatsApp Service: Send via Client

**File:** `src/channels/whatsapp.rs:39-62`

```rust
pub async fn send_to_contact(&self, phone: &str, message: &str) -> Result<String> {
    // STEP 1: Format phone as WhatsApp JID
    let jid_str = format!("{}@s.whatsapp.net", phone);
    let jid = jid_str.parse::<Jid>()?;

    // STEP 2: Create WhatsApp message
    let msg = wa::Message {
        conversation: Some(message.to_string()),
        ..Default::default()
    };

    // STEP 3: Send via whatsapp-rust Client
    let message_id = self.client
        .send_message(jid.clone(), msg)
        .await
        .context("Failed to send WhatsApp message")?;

    info!("✓ Sent WhatsApp message to {}: ID={}", jid_str, message_id);
    Ok(message_id)
}
```

---

## Return Flow: Back to WhatsApp User

**File:** `src/channels/whatsapp.rs:390-406`

```rust
// Tool result goes back through SessionManager
// → SessionManager returns final response
// → Router returns MessageResponse
// → WhatsApp adapter receives response.content

match router.handle_message(&sender, "whatsapp", &text).await {
    Ok(response) => {
        // Create WhatsApp reply
        let reply = wa::Message {
            conversation: Some(response.content),  // "✓ I've sent the message..."
            ..Default::default()
        };

        // Send back to same conversation
        ctx.send_message(reply).await?;
    }
}
```

---

## Data Structures

### ToolDefinition
```rust
// From src/llm/mod.rs
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,  // JSON schema
}
```

### ToolCall
```rust
// From src/llm/mod.rs
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,  // JSON string
}
```

### ChatRequest
```rust
// From src/llm/mod.rs
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub tools: Option<Vec<ToolDefinition>>,  // ← Tools enabled here
}
```

### ChatResponse
```rust
// From src/llm/mod.rs
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub finish_reason: Option<String>,
    pub usage: Option<TokenUsage>,
    pub tool_calls: Option<Vec<ToolCall>>,  // ← Tool calls returned here
}
```

---

## Environment & State

### Global WhatsApp Service
```rust
// From src/lib.rs
static WHATSAPP_SERVICE: OnceCell<Arc<WhatsAppService>> = OnceCell::new();

pub fn set_whatsapp_service(service: Arc<WhatsAppService>) {
    WHATSAPP_SERVICE.set(service).ok();
}

pub fn get_whatsapp_service() -> Option<Arc<WhatsAppService>> {
    WHATSAPP_SERVICE.get().cloned()
}
```

**Initialized in:** `src/channels/whatsapp.rs:388-389`
```rust
let service = Arc::new(WhatsAppService::new(bot.client().clone()));
crate::set_whatsapp_service(service);
```

---

## Execution Timeline (Real Example)

```
T+0ms   User sends WhatsApp: "Send a message to John: Meeting at 3pm"
T+50ms  WhatsApp event received, parsed
T+55ms  Router.handle_message() called
T+60ms  SessionManager creates session: uuid-abc123
T+65ms  User message stored in database
T+70ms  Tools fetched: [send_whatsapp, list_whatsapp_groups]
T+75ms  LLM request built with tools
T+150ms LLM processes (Ollama ~75ms)
T+155ms LLM returns: tool_call(send_whatsapp, {target_type: contact, target: John, message: ...})
T+160ms Tool executor receives tool call
T+165ms WhatsAppService resolves "John" → phone number
T+170ms Client sends WhatsApp message
T+200ms Tool returns: "Message sent (ID: msg_123)"
T+205ms Tool result added to history
T+210ms LLM called again with tool result
T+280ms LLM returns: "✓ I've sent the message to John about the meeting"
T+285ms Response stored in database
T+290ms Response returned to WhatsApp adapter
T+295ms WhatsApp reply sent to user
T+350ms User sees: "✓ I've sent the message to John about the meeting"

TOTAL: ~350ms for full round trip
```

---

## Testing the Flow

### 1. Verify WhatsApp Connection
```bash
# Check bot status
rustyclaw whatsapp status

# See logs
tail -f ~/.rustyclaw/logs/whatsapp.log
```

### 2. Send Test Message
```
Send any message to your WhatsApp bot
Watch the logs for:
  - Message received
  - Session created
  - LLM request sent
  - Response generated
  - Reply sent
```

### 3. Trigger Tool Call
```
Send: "Send a WhatsApp to [contact name]: Test message"

Watch for:
  - LLM decides to use send_whatsapp tool
  - Tool executed with contact lookup
  - Message sent to target
  - Tool result returned
  - Final response composed
```

---

## Debugging

### Enable Debug Logging
```rust
// In config.yaml
logging:
  level: debug
```

### Check Tool Availability
```bash
# Check if WhatsApp service is registered
curl http://localhost:3000/health | jq .services.whatsapp
```

### Monitor Tool Execution
```
RUST_LOG=rustyclaw::tools=trace cargo run
```

