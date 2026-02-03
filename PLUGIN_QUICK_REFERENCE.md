# OpenClaw Plugin System - Quick Reference

## The Core Truth

**OpenClaw does NOT allow the LLM to dynamically create tools.**

Instead:
- Tools are registered at **startup**
- Tools are made available to the LLM as a **fixed set**
- Behavior is customized **dynamically** via:
  - Context-aware factories
  - Hook-based interception
  - Policy-based access control

---

## Quick Facts

| Aspect | Details |
|--------|---------|
| **Plugin Discovery** | At gateway startup (bundled, workspace, global, config) |
| **Plugin Loading** | Via jiti (TypeScript/JS dynamic loader) |
| **Tool Registration** | In plugin's `register(api)` function |
| **Tool Types** | Static definition OR factory function |
| **Factory Context** | sessionKey, workspaceDir, agentId, messageChannel, sandboxed |
| **Hook Execution** | 14 different lifecycle hooks |
| **Hook Types** | Void (parallel), Modifying (sequential), Sync (hot path) |
| **Tool Policy** | Multi-layer: profile, group, global, sandbox |
| **Optional Tools** | Marked `optional: true`, require allowlist |
| **LLM Tool Set** | Fixed at runtime, cannot be changed by LLM |

---

## Plugin Structure

```
my-plugin/
├── manifest.json          (metadata, schema)
├── index.ts              (register function)
└── lib/                  (helpers, utilities)
```

---

## Hook Types & Usage

| Hook | Type | When | Use For |
|------|------|------|---------|
| `before_agent_start` | Modifying | Before LLM prompt | Inject context |
| `agent_end` | Void | After LLM finishes | Log completion |
| `before_compaction` | Void | Before context prune | Observe |
| `after_compaction` | Void | After context prune | Observe |
| `message_received` | Void | User message arrives | Log/notify |
| `message_sending` | Modifying | Before send | Modify/cancel |
| `message_sent` | Void | After send | Log |
| `before_tool_call` | Modifying | Before tool exec | Block/validate |
| `after_tool_call` | Void | After tool exec | Observe |
| `tool_result_persist` | Sync | Before save | Sanitize |
| `session_start` | Void | Session begins | Initialize |
| `session_end` | Void | Session ends | Cleanup |
| `gateway_start` | Void | Gateway starts | Setup |
| `gateway_stop` | Void | Gateway stops | Shutdown |

---

## Tool Registration Patterns

### Pattern 1: Static Tool
```typescript
api.registerTool({
  name: "my-tool",
  description: "Does something",
  parameters: { type: "object", properties: {...} },
  execute: async (id, params) => {
    return { content: [...], details: {} };
  }
});
```

### Pattern 2: Factory Function
```typescript
api.registerTool((ctx: ToolContext) => {
  return {
    name: "context-aware-tool",
    execute: async (id, params) => {
      // Use ctx.sessionKey, ctx.workspaceDir, etc.
      return { content: [...], details: {} };
    }
  };
});
```

### Pattern 3: Optional Tool
```typescript
api.registerTool(tool, { optional: true });
// Only included if in config.plugins.allowlist
```

---

## Hook Registration Pattern

```typescript
api.registerHook("before_agent_start", async (event, ctx) => {
  // event: { prompt, messages }
  // ctx: { agentId, sessionKey, workspaceDir, messageProvider }

  return {
    systemPrompt?: "override",
    prependContext?: "inject this"
  };
}, { name: "my-hook", priority: 1 });
```

---

## Tool Policy Configuration

```yaml
tools:
  allow:
    - exec
    - read
    - web_fetch

  profiles:
    research:
      allow:
        - web_fetch
        - search

  sandbox:
    allow:
      - read
    deny:
      - exec
```

---

## File Locations

| Location | Priority | Origin |
|----------|----------|--------|
| Bundled | Highest | openclaw binary |
| Workspace | High | ~/.openclaw/plugins + project root |
| Global | Medium | ~/.agents/plugins |
| Config | Low | Loaded from config |

---

## Key Interfaces

### Plugin Definition
```typescript
type OpenClawPluginDefinition = {
  id: string;
  name?: string;
  version?: string;
  configSchema?: JsonSchema;
  register: (api: OpenClawPluginApi) => Promise<void>;
};
```

### Tool Context
```typescript
type ToolContext = {
  config: OpenClawConfig;
  workspaceDir?: string;
  agentId?: string;
  sessionKey?: string;
  messageChannel?: string;
  sandboxed?: boolean;
};
```

---

## Common Patterns

### Inject Context Before Agent
```typescript
api.registerHook("before_agent_start", async (event, ctx) => {
  const memory = await loadMemory(ctx.sessionKey);
  return { prependContext: memory };
});
```

### Block Dangerous Tool
```typescript
api.registerHook("before_tool_call", async (event, ctx) => {
  if (event.toolName === "exec" && ctx.sessionKey?.includes("readonly")) {
    return { block: true, blockReason: "Not allowed" };
  }
});
```

### Observe Tool Results
```typescript
api.registerHook("after_tool_call", async (event, ctx) => {
  console.log(`Tool ${event.toolName} took ${event.durationMs}ms`);
});
```

---

## Tool Execution Flow

```
1. Tool factory called with ToolContext
2. Tool added to agent's available tools
3. LLM invoked with tool definitions (fixed set)
4. LLM calls tool:
   - before_tool_call hook
   - tool.execute()
   - after_tool_call hook
   - tool_result_persist hook
   - store result
5. Loop back if more tools needed
```

---

## RustyClaw Implementation

Focus on:
- [ ] Plugin trait with register() method
- [ ] Plugin registry (HashMap)
- [ ] Tool trait and factory function type
- [ ] Hook runner with 14 hook types
- [ ] Tool context struct
- [ ] Tool policy engine
- [ ] Optional tool allowlists
- [ ] Plugin discovery
- [ ] Configuration validation
- [ ] Comprehensive error handling

---

## Key Files in OpenClaw

```
/src/plugins/
├── types.ts (35KB)
├── loader.ts (14KB)
├── registry.ts (14KB)
├── hooks.ts (14KB)
├── tools.ts (3KB)
└── runtime/types.ts (70KB)
```

Total: ~4000 lines

---

## Summary

- Plugin system is **static** (registered at startup)
- Behavior is **dynamic** (via factories, hooks, policies)
- LLM gets **fixed tool set** (cannot create new)
- Hooks enable **flexibility** (14 lifecycle points)
- Policies control **access** (multi-layer, context-aware)

