# OpenClaw Plugin System - Research Summary

## Research Conducted

This research analyzed the OpenClaw repository to understand how plugins enable the LLM to dynamically create and use tools. The investigation covered:

1. **Plugin Type System** - Full TypeScript interfaces from `/src/plugins/types.ts`
2. **Plugin Loading Pipeline** - Complete flow from discovery to execution
3. **Hook System** - All 14 hook types and their execution models
4. **Tool Resolution** - How tools are dynamically loaded per-session
5. **Policy Engine** - Tool access control via multi-layer policies
6. **Agent Integration** - How tools are provided to the LLM
7. **Code Examples** - Real patterns from OpenClaw implementation

---

## Key Finding: OpenClaw Does NOT Support Dynamic Tool Creation

### The Misconception

You might expect that OpenClaw allows the LLM to request new tools or modify existing tools at runtime. This is **not how it works**.

### The Reality

OpenClaw implements a **static plugin registration system** where:

1. **Plugins are discovered at startup** - Not during runtime
2. **Tools are registered once** - At plugin initialization
3. **LLM sees a fixed tool set** - Cannot request new tools
4. **Behavior is customized dynamically** - Via hooks and context

### How OpenClaw Achieves "Dynamic" Behavior

Instead of true dynamic tools, OpenClaw provides:

#### 1. Context-Aware Tool Factories
```typescript
// Tool factory receives session context
api.registerTool((ctx: ToolContext) => {
  return {
    name: "my-tool",
    execute: async (id, params) => {
      // Behavior customized based on:
      // - ctx.sessionKey (user/channel/agent)
      // - ctx.workspaceDir (workspace context)
      // - ctx.sandboxed (execution environment)
      // - ctx.agentId (which agent)
    }
  };
});
```

#### 2. Hook-Based Interception
Plugins can intercept and modify:
- **before_agent_start** - Inject context into LLM prompt
- **before_tool_call** - Validate/block/modify tool parameters
- **after_tool_call** - Process results
- **message_received/sending** - Intercept messages

#### 3. Optional Tool Allowlists
```yaml
# Tools can be marked optional and enabled selectively
plugins:
  allowlist:
    - tool-name
    - plugin-id
    - group:plugins
```

#### 4. Tool Policy Engine
```yaml
tools:
  allow:
    - exec        # Allow by default
    - read
  deny:
    - dangerous   # Deny specific tools

  profiles:
    research:     # Different policy per profile
      allow:
        - web_fetch
        - browser
```

---

## Architecture Overview

### The Plugin Lifecycle

```
Startup:
  Plugin Discovery → Manifest Loading → Module Loading → register() Called
  ↓                  ↓                   ↓                ↓
  Scan dirs         Parse JSON          jiti loads      Tools registered
  Find manifests    Validate schema      TypeScript      Hooks registered

Runtime (per message):
  Tool Resolution → Context Creation → Hook Execution → LLM Invocation
  ↓                 ↓                  ↓               ↓
  Load factories    Create tool ctx    before_agent   LLM calls tools
  Filter optional   with session info  before_tool    from fixed set
```

### The Critical Insight

The LLM **never sees** this complexity. It receives:

1. **A fixed, static list of tools** (known at prompt time)
2. **A prompt** (potentially modified by before_agent_start hooks)
3. **Session history** (conversation context)

The LLM then:
- Calls tools from the fixed set
- Cannot request new tools
- Cannot see behind-the-scenes hooks
- Cannot modify tool definitions

---

## Core Concepts

### 1. Plugin Definition

Every plugin needs:
- **id** - Unique identifier
- **name** - Human-readable name
- **register** - Function that gets called at startup
- **manifest.json** - Metadata and validation schema

### 2. Plugin API (What Plugins Can Do)

Plugins receive `OpenClawPluginApi` with methods to:
- **registerTool()** - Add tools for LLM
- **registerHook()** - Listen to lifecycle events
- **registerCommand()** - Add direct commands (no LLM)
- **registerHttpRoute()** - Add API endpoints
- **registerChannel()** - Add messaging integrations
- **registerProvider()** - Add LLM/STT/TTS providers
- **registerService()** - Register services
- **registerCli()** - Add CLI commands

### 3. Tool Context

Tools created via factory functions receive context:

```typescript
type ToolContext = {
  config: OpenClawConfig;        // Full config
  workspaceDir?: string;         // Agent workspace
  agentId?: string;              // Which agent
  sessionKey?: string;           // User/channel/session
  messageChannel?: string;       // Telegram, Discord, etc.
  agentAccountId?: string;       // Account ID
  sandboxed?: boolean;           // In Docker sandbox?
};
```

This allows tools to customize behavior based on:
- Who's using them (sessionKey)
- Where they're being used (messageChannel)
- What context they have (workspaceDir)
- Whether they can access the full system (sandboxed)

### 4. Hook Types

OpenClaw provides 14 different hooks:

**Agent Hooks** (3):
- `before_agent_start` - Modify prompt before LLM
- `agent_end` - After LLM finishes
- `before_compaction` / `after_compaction` - Context pruning

**Tool Hooks** (3):
- `before_tool_call` - Can block/modify parameters
- `after_tool_call` - Observe results
- `tool_result_persist` - Sanitize before saving

**Message Hooks** (3):
- `message_received` - Incoming message
- `message_sending` - Outgoing message (can modify/cancel)
- `message_sent` - After message sent

**Session/Gateway Hooks** (2):
- `session_start` / `session_end`
- `gateway_start` / `gateway_stop`

Each hook can:
- Modify data (before_* hooks)
- Observe events (after_* hooks)
- Block actions (before_tool_call)
- Add context (before_agent_start)

### 5. Tool Resolution Pipeline

```
resolvePluginTools(context) {
  1. Load all plugin registrations
  2. For each tool factory:
     a. Check if optional
     b. Call factory(context) with session context
     c. Check for name conflicts
     d. Apply tool policies
     e. Add to result array
  3. Return combined tool list
}
```

The **context** allows the same factory to return different tools for different sessions!

Example:
```typescript
api.registerTool((ctx) => {
  if (ctx.sandboxed) {
    return createSandboxedTool();    // Limited
  } else if (ctx.agentId === "admin") {
    return createUnrestrictedTool(); // Full access
  } else {
    return createLimitedTool();      // Medium restrictions
  }
});
```

### 6. Policy Engine

Tools are controlled via multi-layer policies:

```yaml
tools:
  allow:           # Global allow list
    - exec
    - read

  profiles:        # Per-profile policies
    research:
      allow: [web_fetch, search]

  sandbox:         # Sandbox-specific
    allow: [read]
    deny: [exec]

  groups:          # Per-group (Discord channels, Slack workspaces)
    financial:
      allow: [calculator, search]
```

When checking if tool is allowed:
1. Check profile policies (highest priority)
2. Check group policies (channel/workspace specific)
3. Check global policies (default)
4. Check sandbox policies (if running in sandbox)
5. If blocked anywhere: blocked
6. Otherwise: allowed

### 7. Execution Flow

```
Message from User
  ↓
Create session context + tools
  ↓
Run before_agent_start hooks
  ↓
Call LLM with prompt + tools
  ↓
LLM returns tool calls
  ↓
For each tool call:
  ├─ Run before_tool_call hooks (can block)
  ├─ Execute tool
  ├─ Run after_tool_call hooks
  ├─ Run tool_result_persist hook
  └─ Store result in session
  ↓
Return results to LLM (loop back if more tools)
  ↓
LLM returns final response
  ↓
Run agent_end hooks
  ↓
Send response to user
```

---

## Files Referenced

### Core Plugin System (4 files)

| File | Lines | Purpose |
|------|-------|---------|
| `/src/plugins/types.ts` | 35KB | All TypeScript interfaces |
| `/src/plugins/loader.ts` | 14KB | Plugin discovery and loading |
| `/src/plugins/registry.ts` | 14KB | Central registry |
| `/src/plugins/hooks.ts` | 14KB | Hook runner implementation |

### Tool System (3 files)

| File | Lines | Purpose |
|------|-------|---------|
| `/src/agents/pi-tools.ts` | ~1500 | Core tool creation + policy |
| `/src/agents/pi-tool-definition-adapter.ts` | ~200 | LLM tool adaptation |
| `/src/agents/tools/common.ts` | ~300 | Tool utilities |

### Agent Execution (2 files)

| File | Lines | Purpose |
|------|-------|---------|
| `/src/agents/pi-embedded-runner/run/attempt.ts` | ~1000 | Complete agent run |
| `/src/plugins/hook-runner-global.ts` | ~70 | Global hook singleton |

### Total: 9 core files, ~4000 lines of essential code

---

## Patterns for RustyClaw

### Pattern 1: Plugin Registry

```rust
pub struct PluginRegistry {
    pub plugins: HashMap<String, Box<dyn Plugin>>,
    pub tools: Vec<ToolRegistration>,
    pub hooks: Vec<HookRegistration>,
}

pub trait Plugin: Send + Sync {
    fn id(&self) -> &str;
    fn register(&self, api: &PluginApi) -> Result<()>;
}
```

### Pattern 2: Tool Factory

```rust
pub type ToolFactory = dyn Fn(&ToolContext) -> Option<Box<dyn Tool>> + Send + Sync;

pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> &JsonSchema;
    async fn execute(&self, params: JsonValue) -> Result<ToolResult>;
}
```

### Pattern 3: Hook System

```rust
pub type BeforeAgentStartHook =
    dyn Fn(&BeforeAgentStartEvent, &HookContext) -> BoxFuture<Option<BeforeAgentStartResult>>
    + Send + Sync;

pub struct HookRunner {
    hooks: HashMap<String, Vec<Box<BeforeAgentStartHook>>>,
}

impl HookRunner {
    pub async fn run_before_agent_start(
        &self,
        event: &BeforeAgentStartEvent,
        ctx: &HookContext
    ) -> Option<BeforeAgentStartResult> { ... }
}
```

### Pattern 4: Tool Context

```rust
#[derive(Clone)]
pub struct ToolContext {
    pub config: Arc<Config>,
    pub workspace_dir: Option<PathBuf>,
    pub agent_id: Option<String>,
    pub session_key: Option<String>,
    pub message_channel: Option<String>,
    pub sandboxed: bool,
}
```

### Pattern 5: Policy Engine

```rust
pub fn is_tool_allowed(
    tool_name: &str,
    policies: &[&ToolPolicy]
) -> bool {
    // Check each policy in order
    // If any says DENY: false
    // If any says ALLOW: true
    // Default: true
}
```

---

## Common Mistakes to Avoid

### 1. Thinking LLM Creates Tools

**Wrong:** "I'll let the LLM request new tools"
**Right:** "I'll use hooks to modify prompt/behavior"

### 2. Registering Tools Dynamically

**Wrong:** Adding tools in before_agent_start hook
**Right:** Register all tools at startup, use factories for context-awareness

### 3. Blocking All Optional Tools

**Wrong:** Mark all advanced tools as optional
**Right:** Only mark truly optional features as optional

### 4. Ignoring Context in Tools

**Wrong:** Same tool behavior for all sessions/agents
**Right:** Use factory function to customize per context

### 5. Not Using Hooks Properly

**Wrong:** Trying to add tools in before_tool_call
**Right:** Use before_tool_call to validate/block/modify parameters

---

## Performance Considerations

### Tool Resolution
- Called **per-session**, not per-message
- Factories should be lightweight
- Heavy computation should be cached

### Hook Execution
- **Void hooks** run in parallel (agent_end, message_received)
- **Modifying hooks** run sequentially (before_agent_start)
- **Sync hooks** should be fast (tool_result_persist)

### Context Creation
- Created once per message → agent invocation
- Reused for all tool calls in that invocation
- Small object (< 1KB)

---

## Security Model

### Trust Boundaries

1. **Bundled plugins** (highest trust)
   - Shipped with OpenClaw
   - Reviewed by maintainers

2. **Workspace plugins** (medium trust)
   - User's own plugins
   - Can access user's workspace

3. **Global plugins** (lower trust)
   - Shared across agents
   - Can access shared resources

4. **Config plugins** (lowest trust)
   - Loaded from config URLs
   - Limited capabilities

### Sandbox Isolation

Plugins running in sandbox have:
- Limited filesystem (workspace only)
- No network access (unless explicit)
- Resource limits (CPU, memory, processes)

```rust
if context.sandboxed {
    // Use sandbox-aware tool
    return create_sandboxed_tool();
} else {
    // Use unrestricted tool
    return create_full_tool();
}
```

---

## Related Systems in OpenClaw

### 1. Skills System
- Bundled tools per workspace
- Loaded from plugin manifest
- Environment-based activation

### 2. Memory Plugins
- Only one memory backend active (slot system)
- Provides: get/search tools
- Integrated into hooks

### 3. Provider Plugins
- LLM providers (OpenAI, Anthropic, Ollama)
- STT/TTS providers (Whisper, Piper)
- Authentication management

### 4. Channel Plugins
- Message adapters (Telegram, Discord, Slack)
- Custom message actions per channel
- Threading, reactions, formatting

---

## Recommended Reading Order

1. **OPENCLAW_PLUGIN_SYSTEM.md** - Deep technical analysis
2. **PLUGIN_SYSTEM_DIAGRAMS.md** - Visual architecture
3. **PLUGIN_EXAMPLES.md** - Practical code patterns
4. **PLUGIN_RESEARCH_SUMMARY.md** - This document

---

## Questions Answered

### Q1: How does OpenClaw enable LLM to create tools dynamically?
**A:** It doesn't. Tools are registered at startup. Behavior is customized via:
- Context-aware factories
- Hook-based interception
- Optional tool allowlists
- Multi-layer policies

### Q2: What's the plugin architecture?
**A:**
- Discovery → Manifest → Loading → register() call
- Tools registered as factories or static definitions
- Hooks listen to lifecycle events
- Central registry tracks all plugins

### Q3: How does the LLM get tools?
**A:**
1. resolvePluginTools() called with session context
2. Each factory called with context
3. Factories return tools customized for that session
4. Tools converted to LLM format
5. LLM sees fixed tool set (cannot request new ones)

### Q4: What are the key hooks?
**A:**
- `before_agent_start` - Modify prompt
- `before_tool_call` - Block/validate tools
- `after_tool_call` - Observe results
- `message_received/sending` - Intercept messages

### Q5: How are tools controlled?
**A:**
- Global policies (allow/deny)
- Profile-based policies
- Group-based policies (per channel/workspace)
- Sandbox policies
- Optional tools with allowlists

### Q6: What makes tools "dynamic"?
**A:**
- Context-aware factories create different tools per session
- Same factory can return different implementations
- Behavior customized based on user/channel/workspace
- Not true dynamic creation, but "dynamic adaptation"

### Q7: How does security work?
**A:**
- Trust levels by plugin origin
- Sandboxing for untrusted code
- Tool policies control what plugins can do
- Hook validation can block/modify operations

### Q8: What's the performance impact?
**A:**
- Tool resolution happens once per session (not per message)
- Factories should be lightweight
- Hooks run before LLM (can delay slightly)
- Overall impact: negligible for production

---

## Conclusion

OpenClaw's plugin system is a **sophisticated but fundamentally static architecture**:

✓ Tools are registered once at startup
✓ LLM cannot request new tools
✓ Behavior is customized dynamically via factories and hooks
✓ Security enforced via policies and sandboxing
✓ Flexible enough for most use cases

For RustyClaw, adopt:
✓ Plugin registry pattern
✓ Tool factory pattern
✓ Hook system for interception
✓ Policy engine for access control
✓ Context-aware tool creation

Avoid:
✗ Dynamic tool creation (too complex)
✗ Runtime plugin installation (startup only)
✗ Letting LLM define tools (security risk)
✗ Over-complicating the hook system

The elegance of OpenClaw's design is in accepting this constraint and building powerful capabilities around it through hooks, factories, and policies.

