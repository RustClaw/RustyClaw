# WhatsApp Integration Guide

## Overview

RustyClaw integrates with WhatsApp using the [whatsapp-rust](https://github.com/jlucaso1/whatsapp-rust) library, providing multi-channel support for conversations.

## Architecture

```
WhatsApp User
    ↓
WhatsApp Business API / Local Gateway
    ↓
WhatsAppAdapter (WhatsApp channel)
    ↓
Router.handle_message()
    ↓
SessionManager.process_message()
    ├─ Load conversation history (same as Telegram)
    ├─ Auto-route model (code/fast/default)
    ├─ Send to LLM
    └─ Return response
    ↓
Send message back to user on WhatsApp
```

## Implementation Steps

### 1. WhatsApp Setup

**Option A: WhatsApp Business API (Recommended)**

- Register for WhatsApp Business Account
- Get API credentials and phone number
- Set up webhook for message callbacks

**Option B: Local WhatsApp Gateway**

- Use WhatsApp Web automation
- Run local gateway server
- Tunnel to RustyClaw

### 2. Configuration

Add to `config.yaml`:

```yaml
channels:
  whatsapp:
    enabled: true
    phone_number: "1234567890"
    api_key: "${WHATSAPP_API_KEY}"
```

Or environment variable:

```bash
export WHATSAPP_API_KEY="your_api_key"
export WHATSAPP_PHONE="1234567890"
```

### 3. Message Flow

```rust
// User sends message via WhatsApp
WhatsAppAdapter::handle_message(
    from: "1234567890@s.whatsapp.net",
    text: "Hello"
)

// Extracts user ID: "1234567890"
// Routes to: Router::handle_message("1234567890", "whatsapp", "Hello")

// SessionManager creates/loads session scoped by:
// - scope: "per-sender" (user 1234567890)
// - channel: "whatsapp"

// Same conversation history as Telegram!
```

## Key Features

### Session Sharing

Users can switch channels and keep conversation context:

```
Telegram: "Write a function"     → Creates response in qwen2.5:32b
WhatsApp: "Explain that code"    → Sees previous function in context!
```

### Model Routing

Same intelligent routing across all channels:

- **Code task** → deepseek-coder-v2:16b
- **Quick response** → qwen2.5:7b
- **Default** → qwen2.5:32b

### Persistence

All messages stored in SQLite with metadata:

```
messages table:
- user_id:     "1234567890"
- session_id:  "unique-session-uuid"
- channel:     "whatsapp"      ← Identifies WhatsApp channel
- role:        "user" / "assistant"
- content:     Message text
- model_used:  Which model responded
- tokens:      Token count
```

## Implementation Plan

### Phase 1: Basic WhatsApp Integration (Current)

- [x] Add whatsapp-rust dependency
- [x] Create WhatsAppAdapter struct
- [x] Add WhatsApp configuration to schema
- [x] Basic message routing
- [ ] Unit tests for adapter
- [ ] Mock WhatsApp server for testing

### Phase 2: Production WhatsApp Connection (Next)

- [ ] Integrate with WhatsApp Business API / local gateway
- [ ] Webhook server for receiving messages
- [ ] Message sending implementation
- [ ] Error handling and retries
- [ ] Integration tests with real API

### Phase 3: Advanced Features (Future)

- [ ] Media support (images, voice notes)
- [ ] Group chat support
- [ ] Message reactions
- [ ] Typing indicators
- [ ] Read receipts
- [ ] Status updates

## File Structure

```
src/channels/
├── mod.rs             ← Exports WhatsAppAdapter
├── telegram.rs        ← Telegram implementation
└── whatsapp.rs        ← WhatsApp implementation

config.yaml
├── channels:
│   ├── telegram:      ← Telegram bot token
│   └── whatsapp:      ← WhatsApp API credentials
```

## Testing

### Unit Tests

```bash
cargo test --lib channels::whatsapp
```

Tests included:
- Config validation
- User ID extraction from WhatsApp address
- Message handling flow
- Session management

### Integration Tests

```bash
cargo test --test whatsapp_integration -- --ignored
```

Would test:
- Actual WhatsApp API connection
- Message sending/receiving
- Context preservation across channels
- Model routing

## Configuration Examples

### With WhatsApp Business API

```yaml
channels:
  whatsapp:
    enabled: true
    phone_number: "1234567890"
    api_key: "${WHATSAPP_API_KEY}"
```

### Disabled (Default)

```yaml
channels:
  whatsapp:
    enabled: false
```

## Environment Variables

```bash
# .env file or systemd service environment
WHATSAPP_PHONE=1234567890
WHATSAPP_API_KEY=your_api_key_here
```

## Session Scoping

WhatsApp conversations are scoped by:

| Scope Setting | Behavior |
|---------------|----------|
| `per-sender` | One session per phone number across all channels |
| `per-channel-peer` | Separate session per channel per phone number |
| `main` | All WhatsApp users share single session (not recommended) |

Recommended: `per-sender` (same context across Telegram, WhatsApp, Discord)

## Message Format

### Inbound (WhatsApp → RustyClaw)

```json
{
  "from": "1234567890@s.whatsapp.net",
  "text": "Hello, can you help me?"
}
```

User ID extracted: `"1234567890"`

### Outbound (RustyClaw → WhatsApp)

```json
{
  "to": "1234567890",
  "text": "Of course! What do you need help with?"
}
```

## Error Handling

Gracefully handles:

- WhatsApp API timeouts
- Invalid credentials
- Network errors
- Rate limiting

```rust
// Automatic retry with exponential backoff
// Logs errors for monitoring
// Returns user-friendly error messages
```

## Monitoring

### Logs

```bash
# View WhatsApp-related logs
journalctl -u rustyclaw-alpha | grep -i whatsapp

# View all WhatsApp activity
journalctl -u rustyclaw-alpha --grep="WhatsApp"
```

### Database Queries

```sql
-- Messages from WhatsApp users
SELECT COUNT(*) FROM messages
WHERE session_id IN (
  SELECT id FROM sessions WHERE channel = 'whatsapp'
);

-- Average response time by channel
SELECT channel, AVG(tokens) as avg_tokens
FROM messages
GROUP BY channel;
```

## Security Considerations

1. **API Keys**: Never commit API keys, use environment variables
2. **User Data**: Messages stored in SQLite, apply encryption if needed
3. **Rate Limiting**: Implement per-user rate limits to prevent spam
4. **Verification**: Verify webhook signatures from WhatsApp

## Troubleshooting

### WhatsApp messages not received

```bash
# Check adapter is enabled
grep -A 3 "whatsapp:" ~/.rustyclaw/config.yaml

# Verify API key
echo $WHATSAPP_API_KEY

# Check logs
journalctl -u rustyclaw-alpha -p err
```

### Message delivery failures

```bash
# Check network connectivity
curl -X POST https://api.whatsapp.com/... (test API)

# Verify credentials
curl -H "Authorization: Bearer $WHATSAPP_API_KEY" \
     https://graph.whatsapp.com/me
```

### Session issues

```bash
# Check session exists
sqlite3 ~/.rustyclaw/data.db \
  "SELECT * FROM sessions WHERE channel='whatsapp' LIMIT 5;"
```

## Future Enhancements

- [ ] Webhook auto-registration
- [ ] QR code setup for local mode
- [ ] Media handling (documents, images)
- [ ] Group chat support
- [ ] Scheduled messages
- [ ] Message templates
- [ ] Analytics dashboard
- [ ] Multi-account support

## References

- [whatsapp-rust GitHub](https://github.com/jlucaso1/whatsapp-rust)
- [WhatsApp Business API Docs](https://developers.facebook.com/docs/whatsapp/cloud-api)
- [Session Scoping](./proxmox-deployment.md#session-scoping)
- [LLM Routing](./rustyclaw.md#model-routing)
