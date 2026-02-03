# OpenClaw "Agentic Workflow" - Marketing vs Reality

## Executive Summary

**The "agentic workflow" where LLMs dynamically create/generate tools is NOT a real feature in OpenClaw.** This appears to be marketing language around OpenClaw's ability to orchestrate multi-step workflows and its plugin system. The actual implementation has no runtime tool generation, code compilation, or LLM-driven tool creation.

## Investigation Results

### 1. Code Generation & Compilation
**Finding: NONE**

- No `eval()`, `Function()`, or `new Function()` patterns for executing LLM-generated code
- No WASM compilation of LLM output
- No sandboxed code execution from LLM responses
- Browser evaluation (`browser.evaluate`) exists but is NOT driven by LLM-generated schemas

Evidence:
- Search for `eval\|Function\|executeCode|compileWasm` across codebase returned only safe patterns (allowlist evaluation, context guards, browser eval for manual canvas inspection)

### 2. Tool Creation from LLM Requests
**Finding: NONE**

- No mechanism for LLM to request new tools be created at runtime
- No "tool_use" extension allowing LLM to define tool schemas
- No dynamic tool registration triggered by LLM function calls
- Tools are created at startup via `createOpenClawTools()` and plugin system

Key file: `/tmp/openclaw/src/agents/openclaw-tools.ts` (lines 22-132)
- Creates a fixed static array of tools: `browser`, `canvas`, `nodes`, `cron`, `message`, `tts`, `gateway`, `agents_list`, `sessions_list`, `sessions_history`, `sessions_send`, `sessions_spawn`, `session_status`, `web_search`, `web_fetch`, `image`
- Adds plugin tools via `resolvePluginTools()` - but these are also loaded at startup, not created by LLM

### 3. Tool Parameter Schemas
**Finding: STATIC AT STARTUP**

- All tool schemas are TypeBox `Type.Object()` definitions defined at tool creation time
- No tools have dynamic parameter schemas that evolve based on LLM requests
- Lobster workflow tool has fixed schema (action, pipeline, cwd, timeout)
- LLM-task plugin has fixed schema (prompt, input, schema, provider, model, etc.)

### 4. Tool Composition/Orchestration
**Finding: USER-DEFINED + LOBSTER PIPELINES**

OpenClaw has TWO legitimate orchestration mechanisms (both user-driven, NOT LLM-driven):

#### A. Lobster Workflows (Deterministic Pipelines)
- User defines typed pipeline DSL in YAML/JSON or via CLI
- LLM can CALL the Lobster tool but cannot MODIFY or CREATE new pipelines
- Pipelines are data structures (approval gates, resumable state)
- NOT code generation - it's structured shell command orchestration

File: `/tmp/openclaw/docs/tools/lobster.md`

Example Lobster pipeline (fixed, user-defined):
```yaml
steps:
  - id: collect
    command: inbox list --json
  - id: categorize
    command: inbox categorize --json
    stdin: $collect.stdout
  - id: approve
    command: inbox apply
    approval: required
```

The LLM can invoke this via:
```json
{
  "action": "run",
  "pipeline": "/path/to/inbox-triage.lobster",
  "argsJson": "{\"tag\":\"family\"}"
}
```

But the LLM CANNOT create new Lobster definitions at runtime.

#### B. Plugin System (Extensibility at Load Time)
- Plugins register tools via `registerTool()` in `register()` function
- Registration happens at startup, NOT at runtime from LLM requests
- Plugins can define hook handlers that intercept/modify behavior

Files:
- `/tmp/openclaw/src/plugins/types.ts` (OpenClawPluginApi interface)
- `/tmp/openclaw/src/plugins/registry.ts` (Plugin registration)

Example plugin registration (from `/tmp/openclaw/extensions/llm-task/index.ts`):
```typescript
export default function register(api: OpenClawPluginApi) {
  api.registerTool(createLlmTaskTool(api), { optional: true });
}
```

This is called ONCE at startup, not on each LLM turn.

### 5. LLM-Task Tool (The Only "LLM Execution" Tool)
**Finding: RUNS LLM, NOT CREATE TOOLS**

- Optional plugin that lets LLM invoke another LLM call with a prompt and JSON schema validation
- NOT tool creation - it's a "run another model" wrapper
- Disables tools for the nested run (`disableTools: true`)
- Fixed schema:
  ```typescript
  {
    prompt: string,
    input?: unknown,
    schema?: JSONSchema,
    provider?: string,
    model?: string,
    temperature?: number,
    maxTokens?: number,
    timeoutMs?: number
  }
  ```

File: `/tmp/openclaw/extensions/llm-task/src/llm-task-tool.ts`

### 6. Hook System (Interception, Not Creation)
**Finding: HOOKS DO NOT CREATE TOOLS**

Plugin hooks exist for:
- `before_agent_start` - inject context, override system prompt
- `before_tool_call` - intercept tool params
- `after_tool_call` - observe results
- `tool_result_persist` - transform results before persistence
- `message_received/sending/sent` - message pipeline
- `session_start/end` - session lifecycle
- `gateway_start/stop` - gateway lifecycle

File: `/tmp/openclaw/docs/concepts/agent-loop.md`

**None of these hooks allow registering new tools dynamically.** They intercept behavior but cannot modify the tool registry.

### 7. WASM/Sandboxing of LLM Code
**Finding: NONE**

- No WASM runtime for LLM-generated code
- No code generation from LLM responses to WASM
- Docker sandbox exists but is for user-provided code (bash exec), not LLM-generated code
- Browser evaluation is manual (operator chooses when to eval), not LLM-driven code generation

### 8. Function Calling Evolution
**Finding: NONE**

- Tools do NOT evolve or change based on function calls
- Tool schemas are immutable across an agent run
- Only workaround: user restarts gateway with new plugin loaded

## What IS Real: Marketing Language vs Implementation

### What OpenClaw DOES Claim (Marketing)

From README:
- "First-class tools" - TRUE (many tools available)
- "agentic loop" - TRUE (standard agent loop with tool execution)
- "Workflows" - PARTIALLY TRUE (via Lobster user-defined pipelines)
- "Plugin extensibility" - TRUE (at load time)

From Lobster docs:
- "Your assistant can build the tools that manage itself" - MISLEADING
  - Actually: User builds tools/CLIs, Lobster pipelines are user-defined, LLM calls them
  - NOT: LLM generates new tools or Lobster definitions

### What OpenClaw DOESN'T Do

- LLM cannot request new tools be created
- LLM cannot generate code that becomes executable tools
- LLM cannot compose existing tools into new tools at runtime
- LLM cannot define dynamic parameter schemas
- LLM cannot modify tool registry during execution
- Tools cannot evolve based on LLM function calls

## Lobster's Real Purpose (The "Agentic" Hook)

Lobster's actual value is **not** LLM-driven tool creation. It's:

1. **Deterministic pipelines** - Removes token cost of per-step orchestration
2. **Approval gates** - Side effects must be explicitly approved
3. **Resumable state** - Pause/resume without re-running early steps
4. **AI-friendly DSL** - Small grammar prevents "creative" code paths
5. **Constraint enforcement** - Timeouts, output caps, allowlists are baked in

These are legitimate workflow features, but they're all **user-defined at authoring time**, not **generated at runtime by the LLM**.

## Code Evidence

### Static Tool Creation (Never Modified During Run)

File: `/tmp/openclaw/src/agents/openclaw-tools.ts` lines 22-132
```typescript
export function createOpenClawTools(options?: {...}): AnyAgentTool[] {
  const imageTool = options?.agentDir?.trim() ? createImageTool(...) : null;
  const webSearchTool = createWebSearchTool(...);
  const webFetchTool = createWebFetchTool(...);
  const tools: AnyAgentTool[] = [
    createBrowserTool(...),
    createCanvasTool(),
    createNodesTool(...),
    createCronTool(...),
    // ... 20+ more tools, all created at once
  ];
  // Plugin tools added, but also at startup:
  const pluginTools = resolvePluginTools({...}); // Resolved once per session start

  return [...tools, ...pluginTools]; // Static array returned
}
```

### Plugin Registration at Startup Only

File: `/tmp/openclaw/src/plugins/registry.ts`

Types show registration happens once:
```typescript
export type PluginToolRegistration = {
  pluginId: string;
  factory: OpenClawPluginToolFactory;  // Factory called once at startup
  names: string[];
  optional: boolean;
  source: string;
};
```

### LLM-Task is a Wrapper, Not Tool Creation

File: `/tmp/openclaw/extensions/llm-task/src/llm-task-tool.ts`

```typescript
async execute(_id: string, params: Record<string, unknown>) {
  // Just runs another LLM call with schema validation
  // Does NOT create new tools
  const runEmbeddedPiAgent = await loadRunEmbeddedPiAgent();
  // ... run nested LLM with disableTools: true
}
```

## What RustyClaw Should Learn

**Do NOT implement LLM-driven dynamic tool creation** - it's not a real OpenClaw feature.

Instead, implement what OpenClaw actually does:
1. **Fixed tool registry** at startup
2. **Plugin system** for extensibility (load new tools on restart)
3. **Lobster-style workflows** (user-defined deterministic pipelines)
4. **Hook system** for interception (before/after tool calls, etc.)
5. **Static tool schemas** defined once at creation time
6. **Tool composition** at the user/plugin level, NOT LLM level

The "agentic" aspect of OpenClaw is about:
- Running multiple tool calls in a loop
- Streaming tool results
- Formatting function calls appropriately
- Managing session state

NOT about:
- Dynamic code generation
- Runtime tool creation
- LLM-driven tool composition
- Evolving function schemas

## Conclusion

The "agentic workflow" marketing claim is technically accurate but misleading:
- **Accurate**: LLMs can orchestrate multi-step workflows via calling tools
- **Misleading**: Suggests LLMs create/compose tools dynamically (they don't)

This is standard agent loop functionality with good workflow orchestration (Lobster), not a breakthrough in dynamic capability generation.
