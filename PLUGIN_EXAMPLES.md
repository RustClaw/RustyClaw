# OpenClaw Plugin System - Practical Code Examples

This document provides real, working examples of how to build plugins for OpenClaw, with patterns you can adapt for RustyClaw.

---

## Example 1: Simple Static Tool Plugin

```typescript
// my-plugin/manifest.json
{
  "id": "greeting-plugin",
  "name": "Greeting Plugin",
  "version": "1.0.0",
  "description": "Simple greeting tool",
  "configSchema": {
    "parse": (value: unknown) => {
      // Minimal validation
      if (!value || typeof value !== "object") {
        throw new Error("Config must be an object");
      }
      return value;
    }
  }
}

// my-plugin/index.ts
import type { OpenClawPluginDefinition } from "openclaw/plugin-sdk";

export default {
  id: "greeting-plugin",
  name: "Greeting Plugin",
  version: "1.0.0",

  register: async (api) => {
    // Register a simple, static tool
    api.registerTool({
      name: "greet",
      label: "Greet Someone",
      description: "Greets a person by name",
      parameters: {
        type: "object",
        properties: {
          name: {
            type: "string",
            description: "Person's name to greet"
          },
          formal: {
            type: "boolean",
            description: "Use formal greeting",
            default: false
          }
        },
        required: ["name"]
      },
      execute: async (toolCallId, params: Record<string, unknown>) => {
        const name = typeof params.name === "string" ? params.name : "friend";
        const formal = params.formal === true;

        const greeting = formal
          ? `Good day, ${name}. It is a pleasure to meet you.`
          : `Hey ${name}! Nice to see you.`;

        return {
          content: [
            {
              type: "text",
              text: greeting
            }
          ],
          details: {
            greeted: name,
            formal: formal,
            timestamp: new Date().toISOString()
          }
        };
      }
    });

    api.logger.info("Greeting plugin registered");
  }
} satisfies OpenClawPluginDefinition;
```

**How it works:**
1. Define manifest.json with id and schema
2. Export plugin definition with register function
3. In register, call api.registerTool() with tool definition
4. Tool has execute() that LLM can call
5. Returns AgentToolResult with content and details

---

## Example 2: Context-Aware Tool Factory

```typescript
// workspace-plugin/index.ts
import type { OpenClawPluginDefinition } from "openclaw/plugin-sdk";
import fs from "fs/promises";
import path from "path";

export default {
  id: "workspace-plugin",
  name: "Workspace Plugin",
  description: "Tools aware of workspace context",

  register: async (api) => {
    // Register a tool factory that creates context-aware tools
    api.registerTool((ctx) => {
      // ctx contains:
      // - workspaceDir: string (e.g., "/home/user/.openclaw/agents/main")
      // - agentId: string (e.g., "main", "research")
      // - sessionKey: string (e.g., "telegram:user:12345")
      // - messageChannel: string (e.g., "telegram", "discord")
      // - sandboxed: boolean
      // - config: OpenClawConfig

      if (!ctx.workspaceDir) {
        // No workspace - can't provide this tool
        return null;
      }

      return {
        name: "workspace-structure",
        label: "Show Workspace Structure",
        description: `List files in workspace: ${ctx.workspaceDir}`,
        parameters: {
          type: "object",
          properties: {
            path: {
              type: "string",
              description: "Relative path within workspace",
              default: "."
            },
            maxDepth: {
              type: "number",
              description: "Max directory depth",
              default: 2
            }
          }
        },
        execute: async (toolCallId, params: Record<string, unknown>) => {
          const relativePath = typeof params.path === "string" ? params.path : ".";
          const fullPath = path.join(ctx.workspaceDir!, relativePath);

          // Prevent path traversal outside workspace
          const resolved = path.resolve(fullPath);
          const workspaceResolved = path.resolve(ctx.workspaceDir!);
          if (!resolved.startsWith(workspaceResolved)) {
            return {
              content: [{ type: "text", text: "Access denied: path outside workspace" }],
              details: { error: "path_outside_workspace" }
            };
          }

          try {
            const entries = await fs.readdir(resolved, { withFileTypes: true });
            const structure = entries
              .slice(0, 50) // Limit to 50 entries
              .map((entry) => ({
                name: entry.name,
                type: entry.isDirectory() ? "dir" : "file"
              }))
              .sort((a, b) => {
                // Directories first
                if (a.type !== b.type) return a.type === "dir" ? -1 : 1;
                return a.name.localeCompare(b.name);
              });

            return {
              content: [
                {
                  type: "text",
                  text: `Workspace structure for ${ctx.agentId}:\n${structure
                    .map((e) => `${e.type === "dir" ? "[D]" : "[F]"} ${e.name}`)
                    .join("\n")}`
                }
              ],
              details: {
                path: relativePath,
                entries: structure,
                total: entries.length,
                agentId: ctx.agentId,
                workspace: ctx.workspaceDir
              }
            };
          } catch (err) {
            return {
              content: [{ type: "text", text: `Error: ${String(err)}` }],
              details: { error: String(err), path: relativePath }
            };
          }
        }
      };
    }, { optional: false }); // Always include this tool

    api.logger.info("Workspace plugin registered");
  }
} satisfies OpenClawPluginDefinition;
```

**Key patterns:**
1. Tool factory receives context
2. Can return null if conditions not met
3. Can access workspaceDir, agentId, etc.
4. Can customize description based on context
5. Can implement security checks (path traversal)

---

## Example 3: Hook-Based Tool Enhancement

```typescript
// audit-plugin/index.ts
import type { OpenClawPluginDefinition } from "openclaw/plugin-sdk";
import fs from "fs/promises";
import path from "path";

export default {
  id: "audit-plugin",
  name: "Audit Plugin",
  description: "Logs all tool calls for audit trail",

  register: async (api) => {
    const auditDir = path.join(
      api.runtime.state.resolveStateDir(),
      "audit-logs"
    );

    // Create audit directory
    try {
      await fs.mkdir(auditDir, { recursive: true });
    } catch {
      // Ignore if exists
    }

    // Hook: before_tool_call - can block certain tools
    api.registerHook("before_tool_call", async (event, ctx) => {
      // Log the tool call attempt
      const logEntry = {
        timestamp: new Date().toISOString(),
        sessionKey: ctx.sessionKey,
        toolName: event.toolName,
        params: event.params,
        action: "attempted"
      };

      // Write to audit log
      const filename = `${ctx.sessionKey?.replace(/[/:]/g, "_")}_audit.jsonl`;
      const filepath = path.join(auditDir, filename);

      try {
        await fs.appendFile(filepath, JSON.stringify(logEntry) + "\n");
      } catch (err) {
        api.logger.warn(`Failed to write audit log: ${String(err)}`);
      }

      // Block dangerous tools in certain contexts
      if (
        event.toolName === "exec" &&
        ctx.sessionKey?.includes("readonly")
      ) {
        return {
          block: true,
          blockReason: "Execution not allowed in read-only sessions"
        };
      }

      // Block tools not in allowlist for certain agents
      if (ctx.sessionKey?.startsWith("research:")) {
        const allowedTools = ["read", "search_web", "summarize"];
        if (!allowedTools.includes(event.toolName)) {
          return {
            block: true,
            blockReason: `Tool ${event.toolName} not allowed for research agent`
          };
        }
      }

      // Let other tools proceed (don't return anything)
      return;
    }, { name: "audit-before-tool-call" });

    // Hook: after_tool_call - log results
    api.registerHook("after_tool_call", async (event, ctx) => {
      const logEntry = {
        timestamp: new Date().toISOString(),
        sessionKey: ctx.sessionKey,
        toolName: event.toolName,
        result: {
          success: !event.error,
          error: event.error,
          durationMs: event.durationMs
        },
        action: "completed"
      };

      const filename = `${ctx.sessionKey?.replace(/[/:]/g, "_")}_audit.jsonl`;
      const filepath = path.join(auditDir, filename);

      try {
        await fs.appendFile(filepath, JSON.stringify(logEntry) + "\n");
      } catch (err) {
        api.logger.warn(`Failed to write audit log: ${String(err)}`);
      }
    }, { name: "audit-after-tool-call" });

    // Hook: before_agent_start - inject context
    api.registerHook("before_agent_start", async (event, ctx) => {
      // Add reminder about audit logging
      return {
        prependContext: "Note: All your tool calls are logged for audit purposes."
      };
    }, { name: "audit-context" });

    api.logger.info("Audit plugin registered");
  }
} satisfies OpenClawPluginDefinition;
```

**Key patterns:**
1. before_tool_call can block tools with blockReason
2. after_tool_call runs after execution (results ignored)
3. Can implement custom policies per session/agent
4. before_agent_start can inject context via prependContext
5. Multiple hooks can work together

---

## Example 4: Optional Tool with Allowlist

```typescript
// research-plugin/index.ts
import type { OpenClawPluginDefinition } from "openclaw/plugin-sdk";

export default {
  id: "research-plugin",
  name: "Research Tools",
  description: "Advanced research capabilities",

  register: async (api) => {
    // This tool is OPTIONAL - only loaded if explicitly allowed
    api.registerTool(
      (ctx) => {
        return {
          name: "scholarly-search",
          label: "Search Scholarly Articles",
          description: "Search academic databases for peer-reviewed articles",
          parameters: {
            type: "object",
            properties: {
              query: { type: "string", description: "Search query" },
              limit: {
                type: "number",
                default: 10,
                description: "Max results"
              }
            },
            required: ["query"]
          },
          execute: async (toolCallId, params: Record<string, unknown>) => {
            const query = String(params.query || "");

            // In real implementation, would call scholarly API
            // For example: PubMed, arXiv, CrossRef, etc.

            return {
              content: [
                {
                  type: "text",
                  text: `Found 3 scholarly articles for "${query}"\n\n1. Article A (2024)\n2. Article B (2023)\n3. Article C (2022)`
                }
              ],
              details: {
                query: query,
                count: 3,
                source: "scholarly-search-api"
              }
            };
          }
        };
      },
      {
        optional: true, // ← KEY: This tool is optional
        names: ["scholarly-search"] // Alternative names
      }
    );

    api.logger.info("Research plugin registered");
  }
} satisfies OpenClawPluginDefinition;
```

**Configuration to enable it:**

```yaml
# config.yaml
plugins:
  entries:
    research-plugin:
      enabled: true
      config: {}

  # Allowlist optional tools
  allowlist:
    - scholarly-search        # By tool name
    - research-plugin         # By plugin ID
    - group:plugins           # All optional tools (catch-all)

# OR use tool policy:
tools:
  allow:
    - scholarly-search
    - exec
    - read
```

**Key patterns:**
1. Mark optional: true in registerTool options
2. Only loaded if in toolAllowlist config
3. Can be enabled per-session via policy
4. Reduces overhead of unused tools

---

## Example 5: Provider Plugin (LLM/STT/TTS)

```typescript
// llama-provider/manifest.json
{
  "id": "llama-provider",
  "name": "Llama Provider Plugin",
  "version": "1.0.0",
  "description": "Integration with Llama models via ollama"
}

// llama-provider/index.ts
import type { OpenClawPluginDefinition } from "openclaw/plugin-sdk";

export default {
  id: "llama-provider",
  name: "Llama Provider",
  description: "Llama models via ollama",

  register: async (api) => {
    // Register an LLM provider
    api.registerProvider({
      id: "ollama",
      label: "Ollama (Local)",
      docsPath: "/docs/providers/ollama",

      // Available models
      models: {
        "llama2": {
          name: "Llama 2",
          input: ["text"],
          output: ["text"],
          costPer1kTokens: { input: 0, output: 0 }, // Local = free
          contextWindow: 4096,
          maxOutput: 2048
        },
        "neural-chat": {
          name: "Neural Chat",
          input: ["text"],
          output: ["text"],
          costPer1kTokens: { input: 0, output: 0 },
          contextWindow: 4096,
          maxOutput: 2048
        }
      },

      // Authentication method (local = no auth)
      auth: [
        {
          id: "local",
          label: "Local Ollama Instance",
          kind: "token",
          run: async (ctx) => {
            // Verify ollama is running
            try {
              const response = await fetch("http://localhost:11434/api/tags");
              if (!response.ok) {
                throw new Error("Ollama not responding");
              }

              return {
                profiles: [
                  {
                    profileId: "local",
                    credential: {
                      type: "custom",
                      url: "http://localhost:11434"
                    }
                  }
                ],
                notes: ["Connected to local Ollama instance"]
              };
            } catch (err) {
              throw new Error(
                `Cannot connect to Ollama: ${String(err)}. Make sure Ollama is installed and running.`
              );
            }
          }
        }
      ]
    });

    api.logger.info("Llama provider registered");
  }
} satisfies OpenClawPluginDefinition;
```

---

## Example 6: Hook-Based Plugin Composition

```typescript
// memory-plugin/index.ts
import type { OpenClawPluginDefinition } from "openclaw/plugin-sdk";

export default {
  id: "memory-plugin",
  name: "Memory Plugin",
  description: "Automatically saves and retrieves memories",

  register: async (api) => {
    let sessionMemories: Map<string, string[]> = new Map();

    // Hook: before_agent_start - retrieve memories
    api.registerHook("before_agent_start", async (event, ctx) => {
      if (!ctx.sessionKey) return;

      const memories = sessionMemories.get(ctx.sessionKey) || [];

      if (memories.length === 0) return;

      // Inject memories into prompt
      const memoryText = memories
        .map((m, i) => `[Memory ${i + 1}] ${m}`)
        .join("\n");

      return {
        prependContext: `Here are relevant memories from previous sessions:\n${memoryText}`
      };
    }, { name: "retrieve-memories", priority: 1 });

    // Hook: agent_end - save memories
    api.registerHook("agent_end", async (event, ctx) => {
      if (!ctx.sessionKey) return;
      if (!event.success) return; // Only save on success

      // Extract key facts from messages
      const lastMessage = event.messages[event.messages.length - 1];
      if (!lastMessage) return;

      const content = typeof lastMessage.content === "string"
        ? lastMessage.content
        : JSON.stringify(lastMessage.content);

      // Simple heuristic: save sentences longer than 10 words
      const sentences = content.split(/[.!?]+/);
      const meaningful = sentences.filter(
        (s) => s.trim().split(/\s+/).length > 10
      );

      if (meaningful.length === 0) return;

      // Store memory
      const existing = sessionMemories.get(ctx.sessionKey) || [];
      const updated = [...existing, ...meaningful.slice(0, 2)].slice(-10); // Keep last 10
      sessionMemories.set(ctx.sessionKey, updated);

      api.logger.info(
        `Saved ${meaningful.length} memories for session ${ctx.sessionKey}`
      );
    }, { name: "save-memories" });

    api.logger.info("Memory plugin registered");
  }
} satisfies OpenClawPluginDefinition;
```

**How it works:**
1. before_agent_start hook retrieves stored memories
2. Injects them into prompt context
3. agent_end hook extracts key facts from response
4. Stores facts for next session
5. Multiple hooks working together for complex behavior

---

## Example 7: Tool with Dynamic Parameters

```typescript
// command-plugin/index.ts
import type { OpenClawPluginDefinition } from "openclaw/plugin-sdk";

export default {
  id: "command-plugin",
  name: "Command Plugin",
  description: "Custom command execution",

  register: async (api) => {
    // Register a tool that acts like a command processor
    api.registerTool({
      name: "execute-command",
      label: "Execute Command",
      description: "Run a predefined command with arguments",
      parameters: {
        type: "object",
        properties: {
          command: {
            type: "string",
            enum: ["status", "reset", "analyze", "summarize"],
            description: "Command to execute"
          },
          args: {
            type: "object",
            description: "Command-specific arguments",
            additionalProperties: true
          }
        },
        required: ["command"]
      },
      execute: async (toolCallId, params: Record<string, unknown>) => {
        const command = String(params.command || "");
        const args = (params.args as Record<string, unknown>) || {};

        switch (command) {
          case "status":
            return {
              content: [
                {
                  type: "text",
                  text: "System status: All systems operational"
                }
              ],
              details: { command: "status", uptime: "24h 30m" }
            };

          case "reset":
            return {
              content: [
                {
                  type: "text",
                  text: `Reset completed for: ${Object.keys(args).join(", ") || "all"}`
                }
              ],
              details: { command: "reset", affected: args }
            };

          case "analyze":
            const target = args.target;
            return {
              content: [
                {
                  type: "text",
                  text: `Analysis of ${target}:\n- Performance: Good\n- Errors: None\n- Warnings: 2`
                }
              ],
              details: { command: "analyze", target }
            };

          case "summarize":
            const topic = args.topic;
            return {
              content: [
                {
                  type: "text",
                  text: `Summary of ${topic}:\n[Content summary here]`
                }
              ],
              details: { command: "summarize", topic }
            };

          default:
            return {
              content: [
                {
                  type: "text",
                  text: `Unknown command: ${command}`
                }
              ],
              details: { error: "unknown_command", command }
            };
        }
      }
    });

    api.logger.info("Command plugin registered");
  }
} satisfies OpenClawPluginDefinition;
```

---

## Example 8: Complete Real-World Plugin

This example combines multiple patterns:

```typescript
// web-researcher-plugin/manifest.json
{
  "id": "web-researcher",
  "name": "Web Research Plugin",
  "version": "2.0.0",
  "description": "Advanced web research with caching",
  "configSchema": {
    "parse": (value: unknown) => {
      if (typeof value !== "object" || !value) {
        return {};
      }
      const v = value as Record<string, unknown>;
      return {
        apiKey: v.apiKey || process.env.BRAVE_SEARCH_API_KEY || "",
        cacheResults: v.cacheResults !== false,
        maxResults: typeof v.maxResults === "number" ? v.maxResults : 10,
        cacheTtlHours: typeof v.cacheTtlHours === "number" ? v.cacheTtlHours : 24
      };
    }
  }
}

// web-researcher-plugin/index.ts
import type { OpenClawPluginDefinition } from "openclaw/plugin-sdk";
import fs from "fs/promises";
import path from "path";

type SearchResult = {
  title: string;
  description: string;
  url: string;
  timestamp: number;
};

type CacheEntry = {
  results: SearchResult[];
  savedAt: number;
};

export default {
  id: "web-researcher",
  name: "Web Research Plugin",
  description: "Research the web with intelligent caching",

  register: async (api) => {
    const config = (api.pluginConfig || {}) as {
      apiKey?: string;
      cacheResults?: boolean;
      maxResults?: number;
      cacheTtlHours?: number;
    };

    const apiKey = config.apiKey || "";
    const cacheEnabled = config.cacheResults !== false;
    const maxResults = config.maxResults || 10;
    const cacheTtlMs = (config.cacheTtlHours || 24) * 60 * 60 * 1000;

    const cacheDir = path.join(
      api.runtime.state.resolveStateDir(),
      "web-research-cache"
    );

    // Create cache directory
    if (cacheEnabled) {
      try {
        await fs.mkdir(cacheDir, { recursive: true });
      } catch {
        api.logger.warn("Failed to create cache directory");
      }
    }

    // Helper: get cache key from query
    const getCacheKey = (query: string): string => {
      return Buffer.from(query).toString("hex");
    };

    // Helper: read from cache
    const readCache = async (query: string): Promise<SearchResult[] | null> => {
      if (!cacheEnabled) return null;

      try {
        const key = getCacheKey(query);
        const filepath = path.join(cacheDir, `${key}.json`);
        const content = await fs.readFile(filepath, "utf-8");
        const entry = JSON.parse(content) as CacheEntry;

        // Check TTL
        if (Date.now() - entry.savedAt > cacheTtlMs) {
          // Expired
          await fs.unlink(filepath);
          return null;
        }

        return entry.results;
      } catch {
        return null;
      }
    };

    // Helper: write to cache
    const writeCache = async (
      query: string,
      results: SearchResult[]
    ): Promise<void> => {
      if (!cacheEnabled) return;

      try {
        const key = getCacheKey(query);
        const filepath = path.join(cacheDir, `${key}.json`);
        const entry: CacheEntry = {
          results,
          savedAt: Date.now()
        };
        await fs.writeFile(filepath, JSON.stringify(entry, null, 2));
      } catch {
        api.logger.warn("Failed to write cache");
      }
    };

    // Main tool: web search with caching
    api.registerTool(
      (ctx) => {
        return {
          name: "web-search",
          label: "Search the Web",
          description: "Search the web with results caching",
          parameters: {
            type: "object",
            properties: {
              query: {
                type: "string",
                description: "Search query"
              },
              freshOnly: {
                type: "boolean",
                default: false,
                description: "Skip cache and fetch fresh results"
              }
            },
            required: ["query"]
          },
          execute: async (
            toolCallId,
            params: Record<string, unknown>
          ) => {
            const query = String(params.query || "");
            const freshOnly = params.freshOnly === true;

            if (!query.trim()) {
              return {
                content: [{ type: "text", text: "Query is required" }],
                details: { error: "empty_query" }
              };
            }

            // Try cache first
            if (!freshOnly) {
              const cached = await readCache(query);
              if (cached) {
                return {
                  content: [
                    {
                      type: "text",
                      text: `Search results for "${query}" (cached):\n\n${cached
                        .map(
                          (r, i) =>
                            `${i + 1}. **${r.title}**\n   ${r.description}\n   [${r.url}]`
                        )
                        .join("\n\n")}`
                    }
                  ],
                  details: {
                    query,
                    results: cached,
                    fromCache: true,
                    count: cached.length
                  }
                };
              }
            }

            // Fetch from API
            if (!apiKey) {
              return {
                content: [
                  {
                    type: "text",
                    text: "Search API not configured (missing BRAVE_SEARCH_API_KEY)"
                  }
                ],
                details: { error: "no_api_key" }
              };
            }

            try {
              const response = await fetch(
                `https://api.search.brave.com/res/v1/web/search?q=${encodeURIComponent(query)}&count=${maxResults}`,
                {
                  headers: {
                    "Accept": "application/json",
                    "X-Subscription-Token": apiKey
                  }
                }
              );

              if (!response.ok) {
                throw new Error(`API error: ${response.status}`);
              }

              const data = (await response.json()) as {
                web?: Array<{ title: string; description: string; url: string }>;
              };

              const results: SearchResult[] = (data.web || [])
                .slice(0, maxResults)
                .map((r) => ({
                  title: r.title,
                  description: r.description,
                  url: r.url,
                  timestamp: Date.now()
                }));

              // Cache results
              await writeCache(query, results);

              return {
                content: [
                  {
                    type: "text",
                    text: `Search results for "${query}":\n\n${results
                      .map(
                        (r, i) =>
                          `${i + 1}. **${r.title}**\n   ${r.description}\n   [${r.url}]`
                      )
                      .join("\n\n")}`
                  }
                ],
                details: {
                  query,
                  results,
                  fromCache: false,
                  count: results.length
                }
              };
            } catch (err) {
              return {
                content: [
                  {
                    type: "text",
                    text: `Search failed: ${String(err)}`
                  }
                ],
                details: { error: String(err), query }
              };
            }
          }
        };
      },
      { optional: true }
    );

    // Hook: log searches
    api.registerHook("after_tool_call", async (event, ctx) => {
      if (event.toolName === "web-search") {
        api.logger.info(
          `Web search: ${event.params.query} - ${event.error ? "failed" : "succeeded"}`
        );
      }
    });

    api.logger.info("Web researcher plugin registered");
  }
} satisfies OpenClawPluginDefinition;
```

This real-world example demonstrates:
- Configuration validation
- Cache management
- API integration
- Error handling
- Hook integration
- Complex parameter handling
- Logging and debugging

---

## Implementation Checklist for RustyClaw

When implementing these patterns in Rust:

- [ ] **Plugin trait** with register() method
- [ ] **Plugin registry** HashMap<String, Box<dyn Plugin>>
- [ ] **Tool definition** struct with name, description, parameters, execute fn
- [ ] **Hook runner** with before_agent_start, before_tool_call, after_tool_call
- [ ] **Context struct** with session_key, workspace_dir, agent_id, etc.
- [ ] **Tool factory** function type taking Context -> Tool
- [ ] **Optional tools** with allowlist filtering
- [ ] **Policy engine** checking allow/deny lists
- [ ] **Error handling** with Result types
- [ ] **Configuration** loading from YAML
- [ ] **Logging** with structured output
- [ ] **Tests** for plugin loading and execution

