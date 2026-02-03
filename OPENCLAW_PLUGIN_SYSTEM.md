# OpenClaw Plugin System Architecture - Comprehensive Research

Based on detailed analysis of the OpenClaw repository, this document provides an in-depth analysis of how OpenClaw's plugin system enables the LLM to dynamically create and use tools.

## Quick Summary

OpenClaw **does NOT enable the LLM to dynamically create tools**. Instead, it uses a **static plugin registration system** where plugins are:
1. Discovered and loaded at startup
2. Registered with the gateway through a standardized API
3. Made available to the LLM as a fixed set of tools per session

The plugin system provides **dynamic behavior** through hooks and context-aware tool generation, but the tools themselves are predefined at plugin registration time.

---

## 1. Architecture Overview

### Core Components

```
Plugin Discovery → Plugin Loader → Plugin Registry → Hook Runner → Agent Execution
        ↓              ↓                ↓                 ↓              ↓
   filesystem      jiti loader      metadata         before_agent_start   LLM gets
   + manifest      + validation     + tools          + other hooks        tools
```

### Key Files in OpenClaw

- **`/src/plugins/types.ts`** - TypeScript interfaces for plugins (35KB, comprehensive types)
- **`/src/plugins/loader.ts`** - Plugin discovery and loading logic
- **`/src/plugins/tools.ts`** - Tool resolution from plugins
- **`/src/plugins/registry.ts`** - Central registry for all plugin registrations
- **`/src/plugins/hooks.ts`** - Hook runner for plugin lifecycle events
- **`/src/agents/pi-tools.ts`** - Tool creation and policy enforcement for agents
- **`/src/agents/pi-embedded-runner/run/attempt.ts`** - LLM invocation with tool setup

---

## 2. Plugin System Architecture

### 2.1 Plugin Definition Interface

```typescript
// From /src/plugins/types.ts

export type OpenClawPluginDefinition = {
  id: string;
  name?: string;
  version?: string;
  description?: string;
  kind?: PluginKind;  // "memory" | other custom kinds
  configSchema?: OpenClawPluginConfigSchema;
  register: (api: OpenClawPluginApi) => void | Promise<void>;
};

export type OpenClawPluginApi = {
  id: string;
  name: string;
  version?: string;
  description?: string;
  source: string;
  config: OpenClawConfig;
  pluginConfig?: Record<string, unknown>;
  runtime: PluginRuntime;
  logger: PluginLogger;

  // Core registration methods:
  registerTool: (
    tool: AnyAgentTool | OpenClawPluginToolFactory,
    opts?: OpenClawPluginToolOptions,
  ) => void;
  registerHook: (
    events: string | string[],
    handler: InternalHookHandler,
    opts?: OpenClawPluginHookOptions,
  ) => void;
  registerHttpHandler: (handler: OpenClawPluginHttpHandler) => void;
  registerHttpRoute: (params: { path: string; handler: OpenClawPluginHttpRouteHandler }) => void;
  registerChannel: (registration: OpenClawPluginChannelRegistration) => void;
  registerGatewayMethod: (method: string, handler: GatewayRequestHandler) => void;
  registerCli: (registrar: OpenClawPluginCliRegistrar, opts?: { commands?: string[] }) => void;
  registerService: (service: OpenClawPluginService) => void;
  registerProvider: (provider: ProviderPlugin) => void;
  registerCommand: (command: OpenClawPluginCommandDefinition) => void;
  on: <K extends PluginHookName>(
    hookName: K,
    handler: PluginHookHandlerMap[K],
    opts?: { priority?: number },
  ) => void;
};
```

### 2.2 Tool Registration Pattern

Tools can be registered in two ways:

**Option A: Direct Tool Object**
```typescript
api.registerTool({
  name: "my-tool",
  description: "Does something",
  parameters: {
    type: "object",
    properties: {
      input: { type: "string" }
    }
  },
  execute: async (toolCallId, params, signal, onUpdate) => {
    return { content: [{ type: "text", text: "result" }], details: {} };
  }
}, { optional: false });
```

**Option B: Tool Factory (Context-Aware)**
```typescript
api.registerTool((ctx: OpenClawPluginToolContext) => {
  // ctx contains:
  // - config: OpenClawConfig
  // - workspaceDir?: string
  // - agentDir?: string
  // - agentId?: string
  // - sessionKey?: string
  // - messageChannel?: string
  // - agentAccountId?: string
  // - sandboxed?: boolean

  return {
    name: "dynamic-tool",
    description: "Tool with context-aware behavior",
    parameters: { /* ... */ },
    execute: async (toolCallId, params, signal, onUpdate) => {
      // Can use context (sessionKey, agentId, etc.) to customize behavior
      const myPath = ctx.workspaceDir + "/my-file";
      return { /* ... */ };
    }
  };
}, { optional: true });
```

---

## 3. Plugin Loading Lifecycle

### 3.1 Discovery Phase (`/src/plugins/discovery.ts`)

Plugins are discovered from these locations (in order of precedence):

```typescript
1. Bundled plugins (shipped with OpenClaw)
   └─ discovery.candidates[].origin = "bundled"

2. Workspace plugins
   └─ ~/.openclaw/plugins/ or config[].plugins.loadPaths
   └─ discovery.candidates[].origin = "workspace"

3. Global plugins
   └─ ${AGENTS_DIR}/plugins/
   └─ discovery.candidates[].origin = "global"

4. Config-specified plugins
   └─ discovery.candidates[].origin = "config"
```

### 3.2 Manifest Loading

Each plugin directory requires a `manifest.json`:

```json
{
  "id": "my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "description": "Does something useful",
  "kind": "memory",
  "configSchema": {
    // Zod or JSON Schema for plugin configuration
  },
  "configUiHints": {
    "fieldName": {
      "label": "Display Name",
      "help": "Help text",
      "advanced": false,
      "sensitive": false
    }
  },
  "skills": ["./skills/dir1", "./skills/dir2"]
}
```

### 3.3 Plugin Loading Process (`loadOpenClawPlugins` in loader.ts)

```
For each discovered plugin:
  1. Load manifest.json
  2. Validate against config's plugins section
  3. Check if enabled (config.plugins.enabled, slots, etc.)
  4. Load module using jiti (dynamic TypeScript/JS loader)
  5. Parse module export:
     - If function: treat as register handler
     - If object with .register/.activate: use that
     - Otherwise: error
  6. Call plugin.register(api)
     ↓
     Plugin calls api.registerTool(), api.registerHook(), etc.
     ↓
  7. Validate registered tools/hooks
  8. Store in PluginRegistry
```

### 3.4 Tool Resolution (`resolvePluginTools` in /src/plugins/tools.ts)

```typescript
export function resolvePluginTools(params: {
  context: OpenClawPluginToolContext;
  existingToolNames?: Set<string>;
  toolAllowlist?: string[];
}): AnyAgentTool[] {
  // 1. Load all plugins via loadOpenClawPlugins()
  const registry = loadOpenClawPlugins({...});

  // 2. For each plugin's tool factory:
  for (const entry of registry.tools) {
    try {
      // Call the factory with context
      const resolved = entry.factory(params.context);

      // 3. Filter optional tools against allowlist
      if (entry.optional) {
        const allowed = isOptionalToolAllowed({
          toolName: tool.name,
          pluginId: entry.pluginId,
          allowlist: normalizeAllowlist(params.toolAllowlist)
        });
        if (!allowed) continue;
      }

      // 4. Check for conflicts
      if (existing.has(tool.name)) {
        log.error(`tool name conflict: ${tool.name}`);
        continue;
      }

      // 5. Add to result
      tools.push(tool);
    } catch (err) {
      log.error(`plugin tool factory failed (${entry.pluginId}): ${err}`);
    }
  }

  return tools;
}
```

---

## 4. Plugin Hook System

Plugins can inject code at **key lifecycle points** without modifying the LLM's tool definitions.

### 4.1 Hook Categories

**Agent Hooks** (run during LLM invocation):
```typescript
// Called BEFORE the LLM sees the prompt
before_agent_start(event: {
  prompt: string;
  messages?: unknown[];
}, ctx: PluginHookAgentContext): {
  systemPrompt?: string;
  prependContext?: string;  // Prepended to prompt
}

// Called AFTER the LLM finishes
agent_end(event: {
  messages: unknown[];
  success: boolean;
  error?: string;
  durationMs?: number;
}, ctx: PluginHookAgentContext): void;

// Compaction hooks (called when context is pruned)
before_compaction(...): void
after_compaction(...): void
```

**Tool Hooks** (intercept tool execution):
```typescript
// Before tool execution (can block)
before_tool_call(event: {
  toolName: string;
  params: Record<string, unknown>;
}, ctx: PluginHookToolContext): {
  params?: Record<string, unknown>;  // Modified params
  block?: boolean;
  blockReason?: string;
}

// After tool execution
after_tool_call(event: {
  toolName: string;
  params: Record<string, unknown>;
  result?: unknown;
  error?: string;
  durationMs?: number;
}, ctx: PluginHookToolContext): void

// Synchronous hook for session persistence
tool_result_persist(event: {
  toolName?: string;
  toolCallId?: string;
  message: AgentMessage;
  isSynthetic?: boolean;
}, ctx: PluginHookToolResultPersistContext): {
  message?: AgentMessage;
}
```

**Message Hooks** (process incoming/outgoing messages):
```typescript
message_received(event: {
  from: string;
  content: string;
  timestamp?: number;
  metadata?: Record<string, unknown>;
}, ctx: PluginHookMessageContext): void

message_sending(event: {
  to: string;
  content: string;
  metadata?: Record<string, unknown>;
}, ctx: PluginHookMessageContext): {
  content?: string;
  cancel?: boolean;
}

message_sent(event: {
  to: string;
  content: string;
  success: boolean;
  error?: string;
}, ctx: PluginHookMessageContext): void
```

**Session Hooks** (lifecycle events):
```typescript
session_start(event: {
  sessionId: string;
  resumedFrom?: string;
}, ctx: PluginHookSessionContext): void

session_end(event: {
  sessionId: string;
  messageCount: number;
  durationMs?: number;
}, ctx: PluginHookSessionContext): void
```

**Gateway Hooks** (startup/shutdown):
```typescript
gateway_start(event: { port: number }, ctx: PluginHookGatewayContext): void
gateway_stop(event: { reason?: string }, ctx: PluginHookGatewayContext): void
```

### 4.2 Hook Registration Example

```typescript
// In plugin's register function:
api.registerHook(
  ["before_agent_start", "before_tool_call"],
  async (event, ctx) => {
    if (event.toolName === "exec") {
      return {
        block: true,
        blockReason: "exec not allowed in this session"
      };
    }
  },
  { name: "restrict-exec" }
);
```

### 4.3 Hook Execution Model

From `/src/plugins/hooks.ts`:

```typescript
// Void hooks (fire-and-forget, parallel):
agent_end, message_received, message_sending (result), message_sent
→ All handlers run in parallel via Promise.all()

// Modifying hooks (sequential, by priority):
before_agent_start, before_tool_call, message_sending (params)
→ Each handler's result becomes input to next
→ Results merged (e.g., prependContext concatenated with newlines)

// Synchronous hooks (hot path):
tool_result_persist
→ Runs synchronously (no async)
→ Used because session transcripts are appended synchronously
```

---

## 5. LLM Integration: How Tools Are Provided to the LLM

### 5.1 Agent Execution Flow (from `/src/agents/pi-embedded-runner/run/attempt.ts`)

```
runEmbeddedAttempt(params: EmbeddedRunAttemptParams) {
  ↓
  // 1. Create coding tools from OpenClaw's tool factories
  const toolsRaw = createOpenClawCodingTools({
    exec: params.execOverrides,
    sandbox: sandbox,
    messageProvider: params.messageChannel,
    agentAccountId: params.agentAccountId,
    sessionKey: params.sessionKey,
    workspaceDir: effectiveWorkspace,
    config: params.config,
    modelProvider: params.model.provider,
    modelId: params.modelId,
    ...
  });

  ↓
  // 2. Resolve plugin tools using tool context
  const pluginTools = resolvePluginTools({
    context: {
      config: params.config,
      workspaceDir: effectiveWorkspace,
      agentDir: agentDir,
      agentId: params.sessionKey?.split(":")[0],
      sessionKey: params.sessionKey,
      messageChannel: params.messageChannel,
      agentAccountId: params.agentAccountId,
      sandboxed: sandbox?.enabled
    },
    existingToolNames: new Set(toolsRaw.map(t => t.name)),
    toolAllowlist: /* from config.tools.allow or tool policy */
  });

  ↓
  // 3. Combine all tools
  const allTools = [...toolsRaw, ...pluginTools];

  ↓
  // 4. Run before_agent_start hooks
  const hookRunner = getGlobalHookRunner();
  let effectivePrompt = params.prompt;
  if (hookRunner?.hasHooks("before_agent_start")) {
    const hookResult = await hookRunner.runBeforeAgentStart(
      {
        prompt: params.prompt,
        messages: activeSession.messages
      },
      {
        agentId: params.sessionKey?.split(":")[0],
        sessionKey: params.sessionKey,
        workspaceDir: effectiveWorkspace,
        messageProvider: params.messageProvider
      }
    );
    if (hookResult?.prependContext) {
      effectivePrompt = `${hookResult.prependContext}\n\n${params.prompt}`;
    }
  }

  ↓
  // 5. Convert tools to LLM format
  const toolDefinitions = toToolDefinitions(allTools);

  ↓
  // 6. Invoke LLM with prompt, tools, and history
  const response = await streamAgent({
    prompt: effectivePrompt,
    messages: activeSession.messages,
    tools: toolDefinitions,  // ← LLM sees these tools
    ...
  });

  ↓
  // 7. Run tool execution hooks
  for (const toolCall of response.toolCalls) {
    const hookResult = await hookRunner.runBeforeToolCall(
      { toolName: toolCall.name, params: toolCall.params },
      { agentId, sessionKey, toolName: toolCall.name }
    );
    if (hookResult?.block) {
      throw new Error(hookResult.blockReason);
    }
    const updatedParams = hookResult?.params ?? toolCall.params;

    const tool = allTools.find(t => t.name === toolCall.name);
    const result = await tool.execute(toolCall.id, updatedParams, signal, onUpdate);

    await hookRunner.runAfterToolCall({
      toolName: toolCall.name,
      params: updatedParams,
      result: result,
      durationMs: Date.now() - startTime
    }, { agentId, sessionKey, toolName: toolCall.name });
  }
}
```

### 5.2 Tool Schema Conversion

From `/src/agents/pi-tool-definition-adapter.ts`:

```typescript
export function toToolDefinitions(tools: AnyAgentTool[]): ToolDefinition[] {
  return tools.map((tool) => ({
    name: tool.name,
    label: tool.label ?? tool.name,
    description: tool.description ?? "",
    parameters: tool.parameters,  // JSON Schema
    execute: async (...args): Promise<AgentToolResult<unknown>> => {
      const { toolCallId, params, onUpdate, signal } = splitToolExecuteArgs(args);
      try {
        // Call the actual tool
        return await tool.execute(toolCallId, params, signal, onUpdate);
      } catch (err) {
        return jsonResult({
          status: "error",
          tool: tool.name,
          error: err.message
        });
      }
    }
  }));
}
```

---

## 6. Tool Policy System

Plugins can register tools as **optional**, which are only included if explicitly allowed.

### 6.1 Tool Allowlist Configuration

```yaml
# In config.yaml
tools:
  allow:
    # By tool name
    - exec
    - read
    # By plugin ID
    - my-plugin-id
    # By group
    - group:plugins
    - group:fs

  deny:
    - dangerous-tool

  # Profiles for different use cases
  profiles:
    fast:
      allow:
        - exec
        - read
    research:
      allow:
        - web_fetch
        - browser

  # Sandbox-specific policies
  sandbox:
    allow:
      - fs
    deny:
      - network
```

### 6.2 Tool Policy Resolution

From `/src/agents/pi-tools.ts`:

```typescript
function createOpenClawCodingTools(options?: {
  ...
}) {
  // 1. Resolve effective tool policy from multiple sources
  const {
    agentPolicy,      // Agent-specific (/path/to/agent/config.yaml)
    globalPolicy,     // Global (config.yaml)
    profilePolicy,    // Profile (tools.profiles.{profile})
    providerPolicy    // Provider-specific (tools.providers.{provider})
  } = resolveEffectiveToolPolicy({
    config: options?.config,
    sessionKey: options?.sessionKey,
    modelProvider: options?.modelProvider,
    modelId: options?.modelId
  });

  // 2. Resolve group-level policy
  const groupPolicy = resolveGroupToolPolicy({
    config: options?.config,
    groupId: options?.groupId,
    groupChannel: options?.groupChannel,
    senderId: options?.senderId
  });

  // 3. Check if tool is allowed
  const allowBackground = isToolAllowedByPolicies("process", [
    profilePolicy,
    providerPolicy,
    globalPolicy,
    agentPolicy,
    groupPolicy,
    sandbox?.tools,
    subagentPolicy
  ]);

  // 4. Include tool only if allowed
  if (allowBackground) {
    tools.push(createProcessTool({...}));
  }
}
```

---

## 7. The Critical Insight: "Dynamic Tools" ≠ LLM-Created Tools

### 7.1 What OpenClaw Does Provide

✓ **Context-aware tool generation** (via factory functions)
- Tools can customize behavior based on:
  - Current session (`sessionKey`)
  - User/agent identity (`agentId`, `agentAccountId`)
  - Workspace context (`workspaceDir`, `agentDir`)
  - Sandbox state (`sandboxed`)

✓ **Dynamic hook injection** (via plugin hooks)
- Plugins can intercept and modify:
  - System prompt (before_agent_start)
  - Tool parameters (before_tool_call)
  - Tool results (after_tool_call, tool_result_persist)
  - Conversation flow (message_received, message_sending)

✓ **Optional tool loading** (via allowlists)
- Tools can be:
  - Registered as optional
  - Enabled/disabled per session
  - Hidden from LLM unless explicitly allowed

### 7.2 What OpenClaw Does NOT Provide

✗ **Dynamic tool creation by LLM**
- The LLM cannot request new tool definitions at runtime
- All tools must be predefined in plugins or core
- The LLM sees a fixed set of tools per session

✗ **Tool registration after startup**
- Plugin registration happens at gateway startup
- No runtime plugin installation (without restart)
- Tools cannot be added mid-conversation

✗ **LLM-specified tool behavior**
- Tool implementations are fixed at registration
- LLM can't modify tool schemas or signatures
- LLM can't create new tool parameters or return types

---

## 8. How Plugins Enable "Near-Dynamic" Tools

Even without true dynamic tool creation, OpenClaw enables sophisticated tool patterns:

### 8.1 Skills System (`/src/agents/skills/`)

```typescript
// Plugins can register "skills" - directories with predefined tools
// Skills are loaded from plugin manifest:
{
  "skills": ["./skills/research", "./skills/data-analysis"]
}

// Skills are environment-based tools:
// ~/.openclaw/agents/main/skills/research/tools/search.ts
export default {
  name: "search",
  execute: async (params) => {
    // Implementation
  }
}

// OpenClaw discovers and loads these dynamically at run time
// (but they must exist in the filesystem beforehand)
```

### 8.2 Memory Slot System

```typescript
// Plugins can provide different memory implementations
// Only ONE memory plugin is loaded at a time, based on:
{
  "plugins": {
    "slots": {
      "memory": "lancedb"  // Only load the lancedb memory plugin
    }
  }
}

// Different memory backends can provide different capabilities
// but only one is active per session
```

### 8.3 Conditional Tool Generation

```typescript
// A plugin's tool factory can return different tools based on context:
api.registerTool((ctx: OpenClawPluginToolContext) => {
  // Different tools for different sandboxes/workspaces
  if (ctx.sandboxed) {
    return createSandboxedTools();  // Limited capabilities
  } else {
    return createFullTools();       // Full filesystem access
  }
}, { optional: true });
```

### 8.4 Hook-Based Tool Simulation

```typescript
// A plugin can simulate dynamic tools via hooks:
api.registerHook("before_agent_start", async (event, ctx) => {
  // Parse prompt to see what the user is asking for
  if (event.prompt.includes("search_web")) {
    // Inject search capability into system prompt
    return {
      prependContext: "You have access to web search. Use it by calling the search_web tool."
    };
  }
});

// Combined with conditional tool generation:
api.registerTool((ctx) => {
  // Only register search tool if config enables it
  if (ctx.config?.tools?.allow?.includes("search_web")) {
    return createSearchTool();
  }
  return null;
});
```

---

## 9. Plugin Runtime Context

Plugins have access to a rich runtime API (`PluginRuntime`) with 100+ functions:

```typescript
export type PluginRuntime = {
  // Configuration
  config: {
    loadConfig: (path: string) => Promise<OpenClawConfig>;
    writeConfigFile: (path: string, config: OpenClawConfig) => Promise<void>;
  };

  // System operations
  system: {
    enqueueSystemEvent: (event: SystemEvent) => void;
    runCommandWithTimeout: (cmd: string, timeoutSec: number) => Promise<string>;
    formatNativeDependencyHint: (dep: string) => string;
  };

  // Media handling
  media: {
    loadWebMedia: (url: string) => Promise<Buffer>;
    detectMime: (params: {buffer: Buffer}) => Promise<string>;
    getImageMetadata: (path: string) => Promise<ImageMetadata>;
    resizeToJpeg: (params: {...}) => Promise<Buffer>;
  };

  // Tools
  tools: {
    createMemoryGetTool: (params: {...}) => AgentTool;
    createMemorySearchTool: (params: {...}) => AgentTool;
    registerMemoryCli: (program: Command) => void;
  };

  // Channel-specific operations (Discord, Slack, Telegram, WhatsApp, Signal, etc.)
  channel: {
    discord: {
      sendMessageDiscord: (params: {...}) => Promise<void>;
      auditChannelPermissions: (...) => Promise<PermissionAudit>;
      listDirectoryGroupsLive: (...) => Promise<DiscordGroup[]>;
    };
    slack: { /* similar */ };
    telegram: { /* similar */ };
    // ... other channels
  };

  // Logging
  logging: {
    shouldLogVerbose: () => boolean;
    getChildLogger: (bindings?: Record<string, unknown>) => RuntimeLogger;
  };

  // State
  state: {
    resolveStateDir: () => string;
  };
};
```

---

## 10. Plugin Security Model

### 10.1 Trust Boundaries

```
Trusted (bundled):        Built into OpenClaw binary
  ↓ Slightly less trusted
Workspace plugins:        ~/.openclaw/plugins/ + project root
  ↓ Less trusted
Global plugins:           ~/.agents/plugins/
  ↓ Least trusted
Config-loaded plugins:    Dynamically loaded from config URLs
```

### 10.2 Sandbox Integration

Plugins can request sandbox access:

```typescript
api.registerTool((ctx) => {
  if (ctx.sandboxed) {
    // In sandbox: limited filesystem, network isolation
    return createSandboxedTool();
  } else {
    // Not in sandbox: full access
    return createUnrestrictedTool();
  }
});
```

### 10.3 Permission System

Plugin commands require authorization:

```typescript
api.registerCommand({
  name: "sensitive-operation",
  description: "Requires elevated privilege",
  requireAuth: true,  // Only authorized senders can use
  handler: async (ctx) => {
    if (!ctx.isAuthorizedSender) {
      return { content: "Not authorized" };
    }
    // Execute sensitive operation
  }
});
```

---

## 11. Comparison: OpenClaw vs RustyClaw Plugin Architecture

### OpenClaw Plugin System

**Strengths:**
- Comprehensive hook system for intercepting agent behavior
- Context-aware tool generation via factory functions
- Optional tool loading with allowlists
- Plugin configuration with validation
- Rich runtime API with 100+ functions
- Channel-specific integrations
- Skill system for bundled tools

**Limitations:**
- No runtime plugin installation (requires restart)
- LLM cannot create new tools
- Tools must be registered at startup
- All discovered plugins loaded into memory

### RustyClaw Considerations for Rust Port

**Keep from OpenClaw:**
- Hook system for before_agent_start, before_tool_call, after_tool_call
- Context-aware tool factories based on session/agent context
- Optional tool registration with allowlists
- Tool policy engine (allow/deny/elevated)
- Plugin manifest.json validation

**Simplify for RustyClaw:**
- Use static plugin loading (no jiti dynamic loading)
- Require plugin registration at compile time or module load time
- Implement hooks as trait objects or async callbacks
- Use serde for config validation instead of Zod

**Future Enhancements:**
- WASM plugins for untrusted code
- Hot-reload for development (via watchdog)
- Plugin discovery from standard locations (~/.rustyclaw/plugins/)
- Tool caching for performance

---

## 12. Key Takeaways for RustyClaw Implementation

### 12.1 Core Patterns to Adopt

1. **Plugin Registry Pattern** (like OpenClaw)
   - Central registry stores all tools, hooks, commands
   - Plugins register at startup via `register()` function
   - Factory functions for context-aware tool creation

2. **Hook System** (critical for flexibility)
   - `before_agent_start` - inject context, modify prompt
   - `before_tool_call` - validate, block, modify parameters
   - `after_tool_call` - observe, log, modify results
   - `message_received` / `message_sending` - intercept user/agent messages

3. **Tool Context** (enables smart behavior)
   ```rust
   pub struct ToolContext {
     pub config: Arc<Config>,
     pub workspace_dir: Option<PathBuf>,
     pub agent_id: Option<String>,
     pub session_key: Option<String>,
     pub message_channel: Option<String>,
     pub sandboxed: bool,
   }
   ```

4. **Tool Policy Engine** (controls access)
   ```
   allow:        Default allow, deny list exceptions
   deny:         Default deny, allow list exceptions
   elevated:     Requires explicit activation
   ```

### 12.2 Implementation Roadmap

**Phase 1 (Core):**
- [ ] Plugin trait definition
- [ ] Plugin registry (HashMap<String, Box<dyn Plugin>>)
- [ ] Tool registration API
- [ ] Basic hook runner (before_agent_start, before_tool_call, after_tool_call)

**Phase 2 (Enhancement):**
- [ ] Plugin configuration validation (JSON schema or Zod equivalent)
- [ ] Tool allowlist/policy engine
- [ ] Message hooks (message_received, message_sending)
- [ ] Plugin discovery from filesystem

**Phase 3 (Advanced):**
- [ ] Session hooks (session_start, session_end)
- [ ] Gateway hooks (gateway_start, gateway_stop)
- [ ] HTTP route registration
- [ ] CLI command registration
- [ ] Provider plugins (LLM model management)

**Phase 4 (Optional):**
- [ ] WASM plugin runtime
- [ ] Hot-reload support
- [ ] Plugin marketplace/distribution
- [ ] Multi-plugin coordination

---

## 13. Code Example: OpenClaw Plugin

Here's a complete example from OpenClaw patterns:

```typescript
// my-plugin/index.ts
import type { OpenClawPluginDefinition } from "openclaw/plugin-sdk";

export default {
  id: "my-plugin",
  name: "My Plugin",
  version: "1.0.0",
  description: "An example plugin",

  configSchema: {
    // Zod schema for validation
    parse: (value: unknown) => {
      if (typeof value === "object" && value !== null && "apiKey" in value) {
        return value;
      }
      throw new Error("Invalid config");
    }
  },

  async register(api) {
    // 1. Register a static tool
    api.registerTool({
      name: "hello",
      description: "Says hello",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string" }
        }
      },
      execute: async (toolCallId, params) => {
        return {
          content: [{
            type: "text",
            text: `Hello, ${params.name}!`
          }],
          details: { greeting: true }
        };
      }
    });

    // 2. Register a context-aware tool via factory
    api.registerTool((ctx) => {
      return {
        name: "workspace-info",
        description: "Gets workspace information",
        execute: async (toolCallId, params) => {
          const dir = ctx.workspaceDir || process.cwd();
          return {
            content: [{
              type: "text",
              text: `Workspace: ${dir}`
            }],
            details: { workspace: dir }
          };
        }
      };
    }, { optional: true });

    // 3. Register a hook
    api.registerHook("before_agent_start", async (event, ctx) => {
      api.logger.info(`Starting agent in workspace: ${ctx.workspaceDir}`);

      // Inject context into prompt
      return {
        prependContext: `You are working in: ${ctx.workspaceDir}`
      };
    }, { name: "workspace-context" });

    // 4. Register another hook
    api.registerHook("before_tool_call", async (event, ctx) => {
      // Block certain tools in certain contexts
      if (event.toolName === "exec" && ctx.sessionKey?.includes("readonly")) {
        return {
          block: true,
          blockReason: "Execution not allowed in read-only sessions"
        };
      }
    }, { name: "tool-policy" });

    // 5. Register a command (direct user command, no LLM)
    api.registerCommand({
      name: "plugin-status",
      description: "Shows plugin status",
      handler: async (ctx) => {
        return {
          content: "Plugin is active and ready"
        };
      }
    });

    // 6. Access the rich runtime API
    const config = await api.runtime.config.loadConfig(
      await api.runtime.state.resolveStateDir() + "/config.yaml"
    );

    api.logger.info(`Plugin ${api.id} registered with config:`, config);
  }
} satisfies OpenClawPluginDefinition;
```

---

## 14. File Reference Summary

### Core Plugin System Files

| File | Purpose | Lines | Key Content |
|------|---------|-------|-------------|
| `/src/plugins/types.ts` | All TypeScript interfaces | 35KB | Plugin definitions, API, hooks, context types |
| `/src/plugins/loader.ts` | Plugin loading & jiti integration | 14KB | Manifest validation, module loading, caching |
| `/src/plugins/registry.ts` | Central plugin registry | 14KB | Tool/hook/command/service registration |
| `/src/plugins/tools.ts` | Tool resolution logic | 3KB | Factory execution, allowlist filtering, conflict detection |
| `/src/plugins/hooks.ts` | Hook runner implementation | 14KB | Hook execution, handler sequencing, error handling |
| `/src/plugins/runtime.ts` | Plugin runtime globals | 1.8KB | Global singleton for hook runner |
| `/src/plugins/runtime/types.ts` | PluginRuntime API | 70KB | 100+ exported functions for plugins |
| `/src/plugins/commands.ts` | Command registration | 8KB | Plugin-provided direct commands |

### Tool System Files

| File | Purpose | Key Content |
|------|---------|-------------|
| `/src/agents/pi-tools.ts` | Core tool creation | Policy resolution, tool factory composition |
| `/src/agents/pi-tool-definition-adapter.ts` | LLM tool adaptation | Convert AnyAgentTool to LLM ToolDefinition |
| `/src/agents/tools/common.ts` | Tool utilities | Parameter readers, result formatters |

### Agent Execution Files

| File | Purpose | Key Content |
|------|---------|-------------|
| `/src/agents/pi-embedded-runner/run/attempt.ts` | LLM invocation | Full agent execution flow with hooks |

### Integration Files

| File | Purpose | Key Content |
|------|---------|-------------|
| `/src/plugins/hook-runner-global.ts` | Global hook runner | Singleton pattern for hook execution |
| `/src/agents/tools/memory-tool.ts` | Memory plugin tool | Example of specialized tool plugin |
| `/src/agents/skills/plugin-skills.ts` | Skills system | Plugin-provided skill discovery |

---

## 15. Conclusion

OpenClaw's plugin system is **NOT designed for dynamic tool creation by LLMs**. Instead, it provides:

1. **Static plugin registration** at startup
2. **Context-aware tool generation** via factories
3. **Hook-based interception** of agent behavior
4. **Policy-driven tool availability** via allowlists
5. **Runtime API access** for plugins to customize system behavior

The architecture trades dynamic capability for **stability, security, and predictability**. Tools must be predefined, but their behavior can be highly customized through hooks and factory functions.

For RustyClaw, this means:
- Adopt the hook pattern for flexibility
- Use factory functions for context-aware tools
- Implement a policy engine for tool access control
- Keep plugin discovery simple (no jiti equivalent needed)
- Focus on making hooks powerful enough for most "dynamic" use cases

