# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

RustyClaw is a local-first, privacy-focused AI assistant gateway written in Rust. It connects messaging platforms (Telegram, Discord, WhatsApp) to locally-running LLMs (via Ollama, llama.cpp, vLLM) and provides voice processing, plugin support, and code execution sandboxing.

**Key Design Principles:**
- Local-first: All AI processing happens on user's hardware
- Privacy-focused: No cloud dependencies for core features
- OpenClaw-compatible: Similar configuration and feature set for easy migration
- Resource-efficient: Target ~20MB binary, <50MB RAM idle

## Build & Development Commands

### Building
```bash
cargo build                    # Debug build
cargo build --release          # Production build with optimizations
```

### Testing
```bash
cargo test                     # Run all tests
cargo test --lib               # Run library tests only
cargo test integration::      # Run integration tests
cargo test <test_name>        # Run specific test
cargo test -- --nocapture     # Show println! output
```

### Code Quality
```bash
cargo fmt                     # Format code
cargo clippy                  # Run linter
cargo clippy -- -D warnings   # Lint with warnings as errors
```

### Running
```bash
cargo run                     # Run with default config
cargo run -- serve            # Start gateway server
cargo run -- --config path/to/config.yaml serve
cargo watch -x run            # Auto-reload during development
```

### Docker Development
```bash
docker-compose up -d          # Start all services (gateway, ollama, whisper, piper)
docker-compose logs -f rustyclaw  # View gateway logs
docker exec ollama ollama pull qwen2.5:32b  # Pull LLM model
```

## Architecture Overview

### Core Component Interactions

```
User Message → Channel Adapter → Router Engine → Session Manager
                                       ↓
                               Tool Policy Check
                                       ↓
                               LLM Client (via OpenAI-compatible API)
                                       ↓
                               Plugin Runtime (WASM/Python)
                                       ↓
                               Response → Channel Adapter → User
```

### Critical Architecture Patterns

**Session Management:**
- Sessions are scoped by `per-sender`, `main`, `per-peer`, or `per-channel-peer` (configured in config.yaml)
- Session state includes conversation history, context window management, and user preferences
- Sessions can be reset on daily schedule, idle timeout, or explicit command
- Context pruning happens automatically based on token limits (default: 128k tokens)

**LLM Integration:**
- All LLM backends must be OpenAI-compatible (using `/v1/chat/completions` endpoint)
- Model routing allows different models for different task types (code, fast responses, etc.)
- Hot-swap caching keeps multiple models in RAM/SSD for fast switching
- Default endpoint: `http://localhost:11434/v1` (Ollama)

**Plugin System:**
- Dual runtime: WASM (sandboxed, wasmtime) for untrusted code, Python (PyO3) for trusted scripts
- Plugins register handlers via SDK: `on_message`, `command`, `cron`, `on_start`, `on_stop`
- Plugin permissions controlled via manifest.json
- Plugins can access: LLM client, storage, messaging, web fetching (based on permissions)

**Tool Policy Engine:**
- Three access levels: `allow`, `deny`, `elevated`
- Tools grouped by category: `fs`, `web`, `runtime`, `sessions`
- Elevated mode requires explicit user activation (`/elevated on`)
- Policy configured in `tools` section of config.yaml

**Sandbox Execution:**
- Docker-based isolation for code execution
- Scopes: `session` (per user session), `agent` (per agent instance), `shared` (global)
- Workspace modes: `none`, `ro` (read-only), `rw` (read-write)
- Network modes: `none` (isolated), `bridge` (internet access)
- Resource limits: CPU, memory, process count

### Key Module Responsibilities

**`src/config/`** - Multi-format configuration loading (YAML/JSON/TOML), validation, environment variable substitution

**`src/core/`** - Router engine (message routing), session manager (conversation state), request context, global state

**`src/llm/`** - OpenAI-compatible client, model management, routing logic (route by task type), model caching

**`src/voice/`** - TTS (Piper/XTTS integration), STT (Whisper.cpp), audio format conversion

**`src/plugins/`** - WASM runtime (wasmtime), Python runtime (PyO3), plugin loader, API bindings

**`src/channels/`** - Channel trait definition, adapter implementations (Telegram/teloxide, Discord/serenity, WhatsApp/whatsapp-rust, Web/axum)

**`src/tools/`** - Tool policy enforcement, built-in tools (exec, filesystem, web_fetch, web_search, browser automation)

**`src/sandbox/`** - Docker container management, security profiles, workspace mounting

**`src/storage/`** - Storage trait, SQLite backend (default), JSON file backend (OpenClaw-compatible), PostgreSQL backend

**`src/mcp/`** - Model Context Protocol compatibility layer, skills management

## Configuration System

**Config file locations (in priority order):**
1. `--config <path>` CLI argument
2. `~/.rustyclaw/config.yaml`
3. `./config/default.yaml`

**Environment variable substitution:**
- Syntax: `${VAR_NAME}` or `${VAR_NAME:-default}`
- Example: `token: "${TELEGRAM_BOT_TOKEN}"`
- Load from `~/.rustyclaw/secrets.env`

**Key configuration sections:**
- `gateway`: Server host/port, log level
- `llm`: Provider, models, routing rules, cache settings
- `voice`: TTS/STT providers and models
- `channels`: Enable/disable channels, tokens, access control
- `sessions`: Scope, reset policies, context limits
- `tools`: Access policies (allow/deny/elevated)
- `sandbox`: Runtime, security profiles, resource limits
- `plugins`: Enable/disable, paths, runtime settings
- `storage`: Backend type, connection details

## Development Phases

**Phase 1 (Current):** Core foundation - basic gateway with Telegram support, LLM client, SQLite storage
**Phase 2:** Voice & tools - Whisper STT, Piper TTS, web fetch, file system tools, Docker sandbox
**Phase 3:** Plugins & channels - WASM/Python runtimes, Discord, WhatsApp, Web API
**Phase 4:** Advanced features - cron scheduler, MCP compatibility, browser automation, multi-agent routing
**Phase 5:** Mobile & polish - mobile apps, documentation site, plugin marketplace

## Important Conventions

**Error Handling:**
- Use `anyhow::Result` for application errors
- Use `thiserror` for custom error types with context
- Propagate errors with `?` operator, add context with `.context()`

**Async Runtime:**
- All I/O uses Tokio async runtime
- Channel adapters run in separate tasks
- Use `tokio::spawn` for concurrent operations
- Use `tokio::select!` for cancellation and timeouts

**Logging:**
- Use `tracing` crate for structured logging
- Levels: `trace`, `debug`, `info`, `warn`, `error`
- Add spans for request tracking: `#[instrument]` or `tracing::span!`
- Redact sensitive data (tokens, API keys) in logs

**Database Migrations:**
- SQLite migrations in `migrations/sqlite/`
- Use `sqlx migrate run` to apply
- Migrations are versioned: `001_initial.sql`, `002_sessions.sql`, etc.

**Security:**
- Never log sensitive data (tokens, API keys, user messages in production)
- Validate all user inputs before passing to tools
- Use prepared statements for SQL queries
- Sandbox all code execution via Docker
- Implement rate limiting on channel adapters

## OpenClaw Compatibility

RustyClaw aims for configuration compatibility with OpenClaw:
- Similar YAML config structure
- JSON file storage backend option
- Session scoping modes match
- Tool policy syntax compatible
- Workspace structure (AGENTS.md, SOUL.md, TOOLS.md)

When implementing features, refer to OpenClaw documentation for expected behavior.
