# RustyClaw

A local-first, privacy-focused AI assistant gateway written in Rust.

## Overview

RustyClaw connects messaging platforms (Telegram, Discord, WhatsApp) to locally-running LLMs (via Ollama, llama.cpp, vLLM) with built-in voice processing, plugin support, and code execution sandboxing.

**Key Features:**
- **Local-first**: All AI processing happens on your hardware
- **Privacy-focused**: No cloud dependencies for core features
- **Multi-platform**: Supports Telegram, Discord, WhatsApp (future), and Web API
- **Voice support**: Text-to-Speech and Speech-to-Text (planned)
- **Extensible**: Plugin system for community contributions (planned)
- **Resource-efficient**: ~20MB binary target, minimal memory footprint

## Quick Start

### Prerequisites

- Rust 1.70+ (`rustup` recommended)
- Ollama (or another OpenAI-compatible LLM server)
- Telegram Bot Token (from @BotFather)

### Installation

```bash
# Clone the repository
git clone https://github.com/RustClaw/RustyClaw
cd RustyClaw

# Build
cargo build --release

# The binary will be at target/release/rustyclaw
```

### Configuration

1. Create a config file:

```bash
mkdir -p ~/.rustyclaw
cp config/default.yaml ~/.rustyclaw/config.yaml
```

2. Set your environment variables:

```bash
cp .env.example .env
# Edit .env and add your Telegram bot token
export TELEGRAM_BOT_TOKEN="your_token_here"
```

3. Edit `~/.rustyclaw/config.yaml` to match your setup

### Running

```bash
# Start Ollama (in another terminal)
ollama serve

# Pull a model
ollama pull qwen2.5:7b

# Run RustyClaw
cargo run --release -- serve

# Or with a custom config
cargo run --release -- --config /path/to/config.yaml serve
```

## Development

### Build Commands

```bash
cargo build              # Debug build
cargo build --release    # Production build
cargo test              # Run tests
cargo fmt               # Format code
cargo clippy            # Run linter
```

### Project Structure

```
src/
├── config/         # Configuration system
├── core/           # Router and session management
├── llm/            # LLM client integration
├── channels/       # Channel adapters (Telegram, Discord, etc.)
├── storage/        # Database layer (SQLite)
└── main.rs         # CLI entry point
```

## Configuration

See `config/default.yaml` for a full example with all options documented.

Key sections:
- `llm`: LLM provider and models configuration
- `channels`: Enable/disable channels (Telegram, Discord, etc.)
- `sessions`: Session management and scoping
- `storage`: Database backend configuration

## Roadmap

### Phase 1 (Current) ✅
- [x] Core gateway with Telegram support
- [x] LLM client (OpenAI-compatible)
- [x] SQLite storage
- [x] Session management
- [x] Basic CLI

### Phase 2 (Next)
- [ ] Voice processing (Whisper STT, Piper TTS)
- [ ] Built-in tools (web fetch, file system)
- [ ] Docker sandbox for code execution
- [ ] Tool policy engine

### Phase 3
- [ ] Plugin system (WASM + Python)
- [ ] Discord adapter
- [ ] WhatsApp adapter
- [ ] Web API

### Phase 4
- [ ] Cron scheduler
- [ ] MCP compatibility
- [ ] Browser automation
- [ ] Multi-agent routing

### Phase 5
- [ ] Mobile apps
- [ ] Plugin marketplace
- [ ] Documentation site

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

### Development Setup

```bash
git clone https://github.com/RustClaw/RustyClaw
cd RustyClaw
cargo build
cargo test
```

## License

MIT License - See [LICENSE](LICENSE) for details

## Acknowledgments

- Inspired by [OpenClaw](https://openclaw.ai)
- Built with [Tokio](https://tokio.rs), [Teloxide](https://github.com/teloxide/teloxide), and [SQLx](https://github.com/launchbadge/sqlx)
