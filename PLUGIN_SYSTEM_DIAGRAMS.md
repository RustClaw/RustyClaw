# OpenClaw Plugin System - Architecture Diagrams

## 1. Plugin Lifecycle

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Gateway Startup                                 │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Plugin Discovery (discoverOpenClawPlugins)                           │
│                                                                       │
│  Scan locations (in order):                                         │
│  1. Bundled plugins (origin="bundled")                              │
│  2. Workspace plugins (origin="workspace")                          │
│  3. Global plugins (origin="global")                                │
│  4. Config plugins (origin="config")                                │
│                                                                       │
│  Result: discovery.candidates[] with rootDir, source, origin       │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Manifest Loading (loadPluginManifestRegistry)                        │
│                                                                       │
│  For each candidate:                                                │
│  1. Read manifest.json                                              │
│  2. Parse id, name, version, configSchema, skills                  │
│  3. Store metadata in manifestRegistry                             │
│                                                                       │
│  Result: manifestRegistry.plugins[] with metadata                  │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Enable State Resolution (resolveEnableState)                         │
│                                                                       │
│  Check for each plugin:                                             │
│  1. Is it in config.plugins.enabled?                               │
│  2. Is it excluded by config.plugins.disabled?                     │
│  3. Does it fit memory slot (plugins.slots.memory)?                │
│  4. Is there a version conflict?                                   │
│                                                                       │
│  Result: { enabled: boolean, reason?: string }                     │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Module Loading (jiti loader)                                         │
│                                                                       │
│  For each enabled plugin:                                           │
│  1. Load module using jiti(source) - supports .ts, .js, .json     │
│  2. Resolve export:                                                 │
│     - If function: treat as register function                      │
│     - If object with .register/.activate: use that                │
│     - Otherwise: error                                             │
│  3. Create PluginRecord with metadata                              │
│                                                                       │
│  Result: Array<PluginRecord> with loaded module                   │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Plugin Registration (call plugin.register(api))                      │
│                                                                       │
│  Each plugin calls:                                                 │
│  - api.registerTool(tool | factory, options)                       │
│  - api.registerHook(events, handler, options)                      │
│  - api.registerCommand(command)                                     │
│  - api.registerHttpRoute(path, handler)                            │
│  - api.registerChannel(channelPlugin)                              │
│  - api.registerService(service)                                     │
│  - etc.                                                             │
│                                                                       │
│  Result: registry with all tools, hooks, commands indexed         │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Hook Runner Initialization (initializeGlobalHookRunner)              │
│                                                                       │
│  1. Create hook runner from registry                               │
│  2. Set as global singleton                                        │
│  3. Initialize hook dependencies                                   │
│                                                                       │
│  Result: globalHookRunner ready for use                           │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
               Gateway Ready, Awaiting Messages
```

---

## 2. Message Processing to LLM Invocation

```
┌──────────────────────────────────────────────────────────────────────┐
│ User Message Received (via Channel Adapter)                          │
│ Example: Telegram, Discord, WhatsApp, etc.                          │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Route Resolution (resolveRoute)                                      │
│                                                                       │
│  Determine:                                                         │
│  1. Which agent should handle this message                         │
│  2. Session key (per-sender, per-channel-peer, etc.)              │
│  3. Current model/LLM provider                                     │
│  4. Tool policy for this session                                   │
│                                                                       │
│  Result: AgentRoute with target agent, session, model             │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Load Session Context                                                 │
│                                                                       │
│  1. Fetch or create session from storage                            │
│  2. Load conversation history                                       │
│  3. Load session-specific settings                                  │
│                                                                       │
│  Result: SessionManager with message history                       │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Create Tool Set for This Session (createOpenClawCodingTools)         │
│                                                                       │
│  1. Resolve tool policy (global, agent-level, model-level, group)   │
│  2. Create core tools (exec, read, write, etc.)                     │
│  3. Filter by policy (allow/deny/elevated)                          │
│  4. Create context for tool factories:                              │
│     - sessionKey: "telegram:user:123"                              │
│     - workspaceDir: "/home/user/agent"                             │
│     - agentId: "main"                                              │
│     - messageChannel: "telegram"                                    │
│     - sandboxed: true/false                                        │
│                                                                       │
│  Result: Array<AnyAgentTool> - core tools with policy applied      │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Resolve Plugin Tools (resolvePluginTools)                            │
│                                                                       │
│  For each plugin's registered tool:                                │
│  1. Check if optional                                               │
│  2. Call factory(context) if factory function                       │
│  3. Filter optional tools by allowlist                             │
│  4. Check for name conflicts                                        │
│  5. Collect all valid tools                                         │
│                                                                       │
│  Result: Array<AnyAgentTool> - plugin tools with policy applied    │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Combine All Tools                                                    │
│                                                                       │
│  allTools = [coreTool[], pluginTools[]]                            │
│                                                                       │
│  Validate:                                                          │
│  - No duplicate names                                               │
│  - No required parameter conflicts                                 │
│  - Consistent parameter types across similar tools                 │
│                                                                       │
│  Result: Final tool set for this specific session                  │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Run before_agent_start Hooks                                         │
│                                                                       │
│  For each plugin hook listening to "before_agent_start":            │
│  1. Call handler(event, context)                                    │
│  2. Collect results:                                                │
│     - systemPrompt?: string (override)                             │
│     - prependContext?: string (prepend to prompt)                  │
│  3. Merge results:                                                  │
│     effectivePrompt = prependContext + "\n\n" + originalPrompt    │
│                                                                       │
│  Result: Modified prompt with plugin-injected context              │
│                                                                       │
│  Examples:                                                          │
│  - Memory plugin injects recent memories                            │
│  - Workspace plugin injects project context                        │
│  - Analytics plugin adds tracking code                             │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Convert Tools to LLM Format (toToolDefinitions)                      │
│                                                                       │
│  For each AnyAgentTool:                                             │
│  1. Extract: name, description, parameters (JSON Schema)           │
│  2. Wrap execute() with error handling                             │
│  3. Create ToolDefinition compatible with LLM model               │
│                                                                       │
│  Result: Array<ToolDefinition> ready for LLM API                   │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Invoke LLM (streamAgent)                                             │
│                                                                       │
│  Call LLM API with:                                                 │
│  - effectivePrompt                                                  │
│  - session.messages (conversation history)                         │
│  - tools: [ToolDefinition[]]  ← THE FIXED TOOL SET               │
│  - model, temperature, max_tokens, etc.                           │
│                                                                       │
│  Result: LLM response with:                                        │
│  - Text content                                                     │
│  - Tool calls (array of { name, params })                         │
│  - Thinking (if supported)                                         │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
         Tools are FIXED at this point.
    The LLM sees only what was provided.
    LLM CANNOT request new tools or modify schemas.
```

---

## 3. Tool Execution with Hooks

```
                For Each Tool Call in LLM Response:

┌──────────────────────────────────────────────────────────────────────┐
│ Tool Call from LLM: { name: "exec", params: {...} }                 │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Run before_tool_call Hooks                                           │
│                                                                       │
│  For each plugin hook listening to "before_tool_call":              │
│  1. Call handler(event, context)                                    │
│     event: { toolName: "exec", params: {...} }                     │
│     context: { agentId, sessionKey, toolName }                     │
│  2. Collect results:                                                │
│     - params?: Record<string, unknown> (modified)                  │
│     - block?: boolean                                               │
│     - blockReason?: string                                          │
│  3. If blocked:                                                      │
│     Throw error "Tool blocked: {reason}"                           │
│     Return error to LLM                                             │
│  4. Use modified params if provided                                │
│                                                                       │
│  Common uses:                                                       │
│  - Validate parameters against user allowlist                      │
│  - Block certain tools in certain contexts                         │
│  - Sanitize sensitive parameters                                   │
│  - Rate limiting                                                    │
│  - Approval workflow                                                │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Execute Tool                                                         │
│                                                                       │
│  1. Find tool by name in allTools                                   │
│  2. Call tool.execute(toolCallId, params, signal, onUpdate)        │
│  3. Measure duration                                                │
│  4. Catch errors                                                    │
│                                                                       │
│  Result: AgentToolResult<unknown>                                  │
│  {                                                                   │
│    content: [                                                        │
│      { type: "text", text: "..." },                                │
│      { type: "image", data: "...", mimeType: "image/png" }        │
│    ],                                                               │
│    details: { ... }  // Structured result                         │
│  }                                                                   │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Run after_tool_call Hooks                                            │
│                                                                       │
│  For each plugin hook listening to "after_tool_call":               │
│  1. Call handler(event, context)                                    │
│     event: {                                                         │
│       toolName: "exec",                                             │
│       params: {...},                                                │
│       result: AgentToolResult,                                     │
│       durationMs: 1234                                             │
│     }                                                               │
│     context: { agentId, sessionKey, toolName }                     │
│  2. Handler is async, results are not collected                    │
│  3. Errors are logged but don't fail                               │
│                                                                       │
│  Common uses:                                                       │
│  - Logging/audit trail                                             │
│  - Analytics                                                        │
│  - Notification                                                     │
│  - Side effects (save results, update memory)                      │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Run tool_result_persist Hook (SYNCHRONOUS)                           │
│                                                                       │
│  For each plugin hook listening to "tool_result_persist":           │
│  1. Create AgentMessage with tool result                            │
│  2. Call handler(event, context) - MUST BE SYNC                   │
│     event: {                                                         │
│       toolName: "exec",                                             │
│       toolCallId: "...",                                            │
│       message: AgentMessage,                                       │
│       isSynthetic: false                                            │
│     }                                                               │
│     context: { agentId, sessionKey, toolName, toolCallId }        │
│  3. Use returned message if provided (can drop fields)             │
│                                                                       │
│  Common uses:                                                       │
│  - Sanitize message before storing                                 │
│  - Remove sensitive data                                           │
│  - Compress results                                                 │
│  - Add metadata                                                     │
│                                                                       │
│  Note: MUST be synchronous because session transcript is          │
│        appended synchronously (hot path)                           │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Store in Session Transcript                                          │
│                                                                       │
│  Append to SessionManager:                                          │
│  {                                                                   │
│    role: "assistant",                                               │
│    content: [{ type: "tool_result", toolCallId, content: [...] }] │
│  }                                                                   │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
                  Return Tool Result to LLM
                  Continue Agent Loop
```

---

## 4. Hook Execution Model

```
┌─────────────────────────────────────────────────────────────────────┐
│                   Hook Types and Execution                          │
└─────────────────────────┬───────────────────────────────────────────┘

VOID HOOKS (Fire-and-Forget, Parallel Execution)
├─ agent_end
├─ message_received
├─ message_sending (partial - collecting result)
└─ message_sent

Execution:
  hookRunner.runHook("agent_end", event, context)
    ↓
  promises = []
  for handler in handlers:
    promises.push(handler(event, context))
  await Promise.all(promises)  ← All run in parallel
    ↓
  Return (no result collected)

════════════════════════════════════════════════════════════════════════

MODIFYING HOOKS (Sequential, Result-Based)
├─ before_agent_start
├─ before_tool_call
└─ message_sending (params)

Execution:
  hookRunner.runBeforeAgentStart(event, context)
    ↓
  result = {}
  for handler in handlers.sort(by priority):
    hookResult = await handler(event, context)
    if hookResult:
      // Merge results
      result.systemPrompt ??= hookResult.systemPrompt
      result.prependContext += hookResult.prependContext + "\n\n"
    event = { ...event, ...hookResult }  ← Next handler sees modified event
    ↓
  Return merged result
    ↓
  Caller merges results into final value

════════════════════════════════════════════════════════════════════════

SYNCHRONOUS HOOK (Hot Path)
└─ tool_result_persist

Execution:
  hookRunner.runToolResultPersist(event, context)
    ↓
  message = event.message
  for handler in handlers.sort(by priority):
    const handlerResult = handler(event, context)  ← NO await
    if handlerResult?.message:
      message = handlerResult.message
    ↓
  Return message

Note: MUST be synchronous because called during session save
      (hot path where every ms matters)
```

---

## 5. Tool Policy Resolution

```
┌─────────────────────────────────────────────────────────────────────┐
│         Tool Policy Resolution (isToolAllowedByPolicies)            │
└──────────────────────────┬────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Check Multiple Policy Layers (in order)                             │
│                                                                     │
│  For tool "exec", check if allowed in:                            │
│                                                                     │
│  1. Profile Policy (tools.profiles.{profile}.allow/deny)          │
│     └─ If match found (allow/deny), use that                      │
│                                                                     │
│  2. Provider Profile Policy                                        │
│     └─ (tools.providers.{provider}.allow/deny)                    │
│     └─ If match found (allow/deny), use that                      │
│                                                                     │
│  3. Global Policy (tools.allow/deny at root)                       │
│     └─ Default policy for all agents/sessions                      │
│     └─ If match found (allow/deny), use that                      │
│                                                                     │
│  4. Agent Policy (agentDir/config.yaml tools.allow/deny)           │
│     └─ Specific to this agent instance                            │
│     └─ If match found (allow/deny), use that                      │
│                                                                     │
│  5. Group Policy (config.groups.{group}.tools)                     │
│     └─ Specific to Discord channel, Slack group, etc.             │
│     └─ If match found (allow/deny), use that                      │
│                                                                     │
│  6. Sandbox Policy (sandbox config)                                │
│     └─ If running in Docker sandbox, check sandbox.tools          │
│     └─ If match found (allow/deny), use that                      │
│                                                                     │
│  7. Subagent Policy (if spawned by parent)                         │
│     └─ Parent may restrict child agent tools                       │
│     └─ If match found (allow/deny), use that                      │
│                                                                     │
│  If ANY layer says DENY: Tool is NOT allowed                      │
│  If ANY layer says ALLOW: Tool is allowed (unless later denied)   │
│  If NO layer specifies: Tool is allowed (default allow)           │
└──────────────────────────┬────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Optional Tools (Additional Filtering)                               │
│                                                                     │
│  If tool is marked optional in plugin:                             │
│  1. Check toolAllowlist from config                                │
│  2. Allow if:                                                       │
│     - Tool name is in allowlist, OR                               │
│     - Plugin ID is in allowlist, OR                               │
│     - "group:plugins" is in allowlist                             │
│  3. Otherwise: Skip tool (don't include)                          │
│                                                                     │
│  This lets plugins offer tools that aren't enabled by default    │
└──────────────────────────┬────────────────────────────────────────┘
                           │
                           ▼
                      Tool Is Allowed (✓) or Not (✗)
                      Tool is included or excluded
```

---

## 6. Plugin Module Export Patterns

```
┌─────────────────────────────────────────────────────────────────────┐
│     How Plugins Export Their Definition                             │
└──────────────────────────┬────────────────────────────────────────┘

PATTERN 1: Default Export Function
───────────────────────────────────
// index.ts
export default async (api: OpenClawPluginApi) => {
  api.registerTool({...});
  api.registerHook([...], ...);
}

Used as: plugin.register = exported function
        If no .register, uses .activate, else error

════════════════════════════════════════════════════════════════════════

PATTERN 2: Default Export Object with register()
─────────────────────────────────────────────────
// index.ts
export default {
  id: "my-plugin",
  name: "My Plugin",
  register: async (api) => {
    api.registerTool({...});
  }
}

Resolved as: plugin.definition = exported object
            plugin.register = definition.register or definition.activate

════════════════════════════════════════════════════════════════════════

PATTERN 3: Named Exports
──────────────────────────
// index.ts
export const register = async (api: OpenClawPluginApi) => {
  api.registerTool({...});
}

Then import with: import * as module; module.register

════════════════════════════════════════════════════════════════════════

PATTERN 4: CommonJS
──────────────────
// index.js
module.exports = async (api) => {
  api.registerTool({...});
}

or

module.exports = {
  register: async (api) => {...}
}

Both supported via jiti's interop

════════════════════════════════════════════════════════════════════════

LOADING PROCESS (resolvePluginModuleExport)
─────────────────────────────────────────────
1. Load module: mod = jiti(source)
2. Check if has .default: (mod as {default}).default
3. If .default is function: use as register directly
4. If .default is object: use .default.register or .default.activate
5. If no .default: use module as-is
6. Final check: if resolved is function, use as register
              if resolved is object, extract .register or .activate
```

---

## 7. Plugin Registry Structure

```
┌──────────────────────────────────────────────────────────────┐
│              PluginRegistry (Central Hub)                   │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│ plugins: PluginRecord[]                                     │
│ ├─ id: string                                               │
│ ├─ name: string                                             │
│ ├─ version?: string                                         │
│ ├─ enabled: boolean                                         │
│ ├─ status: "loaded" | "disabled" | "error"                │
│ ├─ toolNames: string[]                                      │
│ ├─ hookNames: string[]                                      │
│ ├─ channelIds: string[]                                     │
│ ├─ providerIds: string[]                                    │
│ ├─ configSchema: boolean                                    │
│ └─ ... other metadata                                       │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│ tools: PluginToolRegistration[]                             │
│ ├─ pluginId: string                                         │
│ ├─ factory: (ctx) => AnyAgentTool | AnyAgentTool[]         │
│ ├─ names: string[] (declared names)                         │
│ ├─ optional: boolean                                        │
│ └─ source: string (file path)                              │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│ hooks: PluginHookRegistration[]                             │
│ ├─ pluginId: string                                         │
│ ├─ events: string[] ("before_agent_start", etc.)           │
│ ├─ handler: InternalHookHandler                            │
│ ├─ priority?: number                                        │
│ └─ source: string                                           │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│ commands: PluginCommandRegistration[]                       │
│ ├─ pluginId: string                                         │
│ ├─ command: OpenClawPluginCommandDefinition                │
│ │  ├─ name: string                                          │
│ │  ├─ description: string                                   │
│ │  ├─ acceptsArgs?: boolean                                │
│ │  ├─ requireAuth?: boolean                                │
│ │  └─ handler: (ctx) => PluginCommandResult               │
│ └─ source: string                                           │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│ channels: PluginChannelRegistration[]                       │
│ ├─ pluginId: string                                         │
│ ├─ plugin: ChannelPlugin                                    │
│ └─ source: string                                           │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│ providers: ProviderPluginRegistration[]                     │
│ ├─ pluginId: string                                         │
│ ├─ provider: ProviderPlugin (LLM, STT, TTS)                │
│ └─ source: string                                           │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│ httpHandlers: PluginHttpRegistration[]                      │
│ ├─ pluginId: string                                         │
│ ├─ handler: (req, res) => boolean                          │
│ └─ source: string                                           │
│                                                              │
│ httpRoutes: PluginHttpRouteRegistration[]                   │
│ ├─ path: string (e.g., "/plugin/my-plugin/action")        │
│ ├─ handler: (req, res) => void                             │
│ └─ source: string                                           │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│ gatewayHandlers: GatewayRequestHandlers                      │
│ ├─ Key: method name (e.g., "agent/list")                  │
│ └─ Value: handler function                                 │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│ services: PluginServiceRegistration[]                       │
│ ├─ pluginId: string                                         │
│ ├─ service: OpenClawPluginService                          │
│ └─ source: string                                           │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│ diagnostics: PluginDiagnostic[]                             │
│ ├─ level: "warn" | "error"                                 │
│ ├─ message: string                                          │
│ ├─ pluginId?: string                                        │
│ └─ source?: string                                          │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

---

## 8. Context-Aware Tool Factory Example

```
┌──────────────────────────────────────────────────────────┐
│         Tool Factory with Context Awareness             │
└────────────────────┬─────────────────────────────────────┘
                     │
                     ▼
    api.registerTool((ctx: ToolContext) => {
      return {
        name: "workspace-action",
        execute: async (toolCallId, params, signal, onUpdate) => {
          // Factory runs at tool resolution time (per-session)
          // Can access context:

          console.log(`Session: ${ctx.sessionKey}`);
          // "telegram:user:12345" or "discord:guild:123:ch:456"

          console.log(`Workspace: ${ctx.workspaceDir}`);
          // "/home/user/.openclaw/agents/main"

          console.log(`Agent: ${ctx.agentId}`);
          // "main" or "research-agent"

          console.log(`Channel: ${ctx.messageChannel}`);
          // "telegram", "discord", "slack", "whatsapp", etc.

          console.log(`Sandboxed: ${ctx.sandboxed}`);
          // true if running in Docker sandbox

          console.log(`Account: ${ctx.agentAccountId}`);
          // Account ID (for multi-account setups)

          // Different behavior based on context:
          if (ctx.sandboxed) {
            // Limited filesystem access
            const safeDir = ctx.workspaceDir + "/safe";
            // Only allow operations in /safe
          } else if (ctx.messageChannel === "telegram") {
            // Telegram-specific behavior
            // Can mention user by Telegram ID
          } else if (ctx.agentId === "financial-agent") {
            // Financial agent specific behavior
            // Different rate limits or permissions
          }

          // Execute tool with context-aware behavior
          const result = await performAction(params, ctx);
          return {
            content: [{ type: "text", text: result }],
            details: { context: ctx }
          };
        }
      };
    }, { optional: true });
                     │
                     ▼
    At Runtime (when tools needed):
    ┌──────────────────────────────────┐
    │  resolvePluginTools({            │
    │    context: {                    │
    │      sessionKey: "telegram:...   │
    │      workspaceDir: "/home/user..." │
    │      agentId: "main",            │
    │      messageChannel: "telegram", │
    │      sandboxed: false            │
    │    },                            │
    │    toolAllowlist: ["workspace-*"] │
    │  })                              │
    └──────────────┬───────────────────┘
                   │
                   ▼
    ┌──────────────────────────────────┐
    │  For each tool factory:          │
    │  1. Call factory(context)        │
    │  2. Check if optional/allowed    │
    │  3. Add to tools array           │
    │  4. Tool now has context-aware   │
    │     behavior for this session    │
    └──────────────────────────────────┘
```

---

## 9. Hook Execution Sequence During Agent Run

```
                     ┌─────────────────────────┐
                     │   LLM Invocation        │
                     │  runEmbeddedAttempt()   │
                     └────────────┬────────────┘
                                  │
                ┌─────────────────▼───────────────────┐
                │ 1. Load Session + Tools             │
                │    - Load conversation history      │
                │    - Create tool set                │
                │    - Resolve plugin tools           │
                └────────────┬────────────────────────┘
                             │
                ┌────────────▼──────────────────────┐
                │ 2. Run before_agent_start Hooks   │
                │ ┌────────────────────────────────┐│
                │ │ Plugin A: Inject memories      ││
                │ │ Plugin B: Add project context  ││
                │ │ Plugin C: Track session        ││
                │ └────────────────────────────────┘│
                │ Result: Modified prompt            │
                └────────────┬─────────────────────┘
                             │
                ┌────────────▼──────────────────────┐
                │ 3. Convert to LLM Format           │
                │ - Create ToolDefinitions[]        │
                │ - LLM sees fixed tool set         │
                └────────────┬─────────────────────┘
                             │
                ┌────────────▼──────────────────────┐
                │ 4. Invoke LLM API                  │
                │ streamAgent({                      │
                │   prompt,                         │
                │   messages,                       │
                │   tools: [exec, read, search, ...],
                │   model,                          │
                │   ...                             │
                │ })                                │
                │ ┌────────────────────────────────┐│
                │ │ LLM Response:                  ││
                │ │ - Text: "I'll search for..."  ││
                │ │ - Tool: search_web             ││
                │ │ - Tool: read_file              ││
                │ └────────────────────────────────┘│
                └────────────┬─────────────────────┘
                             │
    ┌────────────────────────▼──────────────────────────────────────┐
    │ 5. For Each Tool Call (sequential):                           │
    │                                                               │
    │ First Tool Call: { name: "search_web", params: {...} }       │
    │ ┌─────────────────────────────────────────────────────────┐  │
    │ │ 5a. Run before_tool_call Hooks                         │  │
    │ │  - Plugin A: Check rate limits (block? modify params?) │  │
    │ │  - Plugin B: Sanitize sensitive params                │  │
    │ │  Result: Modified params or blocked                   │  │
    │ └─────────────────────────────────────────────────────────┘  │
    │ ┌─────────────────────────────────────────────────────────┐  │
    │ │ 5b. Execute Tool                                       │  │
    │ │  - tool.execute(id, params, signal, onUpdate)          │  │
    │ │  Result: AgentToolResult                               │  │
    │ └─────────────────────────────────────────────────────────┘  │
    │ ┌─────────────────────────────────────────────────────────┐  │
    │ │ 5c. Run after_tool_call Hooks (parallel)              │  │
    │ │  - Plugin A: Log result to analytics                   │  │
    │ │  - Plugin B: Notify user of action                     │  │
    │ │  - Plugin C: Update session metadata                   │  │
    │ │  (Results ignored, runs in parallel)                   │  │
    │ └─────────────────────────────────────────────────────────┘  │
    │ ┌─────────────────────────────────────────────────────────┐  │
    │ │ 5d. Run tool_result_persist Hook (sync)               │  │
    │ │  - Plugin A: Sanitize sensitive data before saving      │  │
    │ │  - Plugin B: Add metadata to message                    │  │
    │ │  Result: Modified message to store                     │  │
    │ └─────────────────────────────────────────────────────────┘  │
    │ ┌─────────────────────────────────────────────────────────┐  │
    │ │ 5e. Store in Session Transcript                        │  │
    │ │  - Add assistant message with tool_result              │  │
    │ └─────────────────────────────────────────────────────────┘  │
    │                                                               │
    │ [Repeat for each tool call: read_file, exec, etc.]           │
    └─────────────────────────┬──────────────────────────────────┘
                              │
                ┌─────────────▼──────────────────────┐
                │ 6. Return to LLM with Results      │
                │ - Tool 1 succeeded: {...}         │
                │ - Tool 2 blocked: Error message   │
                │ - Tool 3 succeeded: {...}         │
                └─────────────┬────────────────────┘
                              │
                ┌─────────────▼──────────────────────┐
                │ 7. LLM Generates Next Response     │
                │ (Uses tool results to refine)      │
                │ - Continues loop if more tools     │
                │ - Returns final text if done       │
                └─────────────┬────────────────────┘
                              │
                ┌─────────────▼──────────────────────┐
                │ 8. Run agent_end Hook              │
                │ ┌────────────────────────────────┐│
                │ │ Plugin A: Log completion      ││
                │ │ Plugin B: Update memory       ││
                │ │ Plugin C: Clean up resources  ││
                │ └────────────────────────────────┘│
                │ (Runs in parallel, results unused) │
                └─────────────┬────────────────────┘
                              │
                ┌─────────────▼──────────────────────┐
                │ 9. Return Response to User         │
                │ - Via channel adapter              │
                │ - Telegram, Discord, etc.          │
                └────────────────────────────────────┘
```

This is the **complete lifecycle** of how tools are created, filtered, executed, and hooks are invoked during a single message-to-response cycle.

