# Proxmox Alpha Testing Setup

This document describes how to set up the Proxmox server for alpha testing with GitHub Actions self-hosted runner.

## Prerequisites

- Proxmox server with a VM or LXC container for testing
- SSH access to the Proxmox server
- Ollama or compatible LLM server running on the same network

## Self-Hosted Runner Setup

### 1. Create Runner on GitHub

1. Go to your organization settings: `https://github.com/organizations/RustClaw/settings/actions/runners`
2. Click "New self-hosted runner"
3. Select Linux as the OS
4. Follow the instructions to download and configure the runner

### 2. Install on Proxmox

SSH into your Proxmox server and run:

```bash
# Create a directory for the runner
mkdir -p ~/actions-runner && cd ~/actions-runner

# Download the latest runner package
curl -o actions-runner-linux-x64-2.311.0.tar.gz -L \
  https://github.com/actions/runner/releases/download/v2.311.0/actions-runner-linux-x64-2.311.0.tar.gz

# Extract the installer
tar xzf ./actions-runner-linux-x64-2.311.0.tar.gz

# Configure the runner (use the URL and token from GitHub)
./config.sh --url https://github.com/RustClaw/RustyClaw --token YOUR_TOKEN_HERE

# Install and start the service
sudo ./svc.sh install
sudo ./svc.sh start
```

### 3. Install Dependencies

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install system dependencies
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev

# Install Ollama (if not already installed)
curl -fsSL https://ollama.com/install.sh | sh

# Pull a model
ollama pull qwen2.5:7b
```

### 4. Configure Environment

Create a config directory and environment file:

```bash
mkdir -p ~/rustyclaw-alpha/config
```

Create `~/rustyclaw-alpha/config/config.yaml`:

```yaml
llm:
  base_url: "http://localhost:11434/v1"
  models:
    primary: "qwen2.5:7b"

channels:
  telegram:
    enabled: true
    token: "${TELEGRAM_BOT_TOKEN}"
    allowed_users: []  # Add your Telegram user ID here

sessions:
  scope: "per-sender"
  max_tokens: 128000

storage:
  storage_type: "sqlite"
  path: "/home/YOUR_USER/rustyclaw-alpha/data.db"

logging:
  level: "info"
  format: "pretty"
```

Create `~/rustyclaw-alpha/.env`:

```bash
TELEGRAM_BOT_TOKEN=your_bot_token_here
```

### 5. Test Deployment

Push to the main branch to trigger deployment:

```bash
git push origin main
```

Check the runner status:
```bash
sudo systemctl status rustyclaw-alpha
```

View logs:
```bash
sudo journalctl -u rustyclaw-alpha -f
```

## Manual Deployment

If you need to deploy manually:

```bash
cd ~/RustyClaw  # or wherever you cloned the repo
git pull
cargo build --release
cp target/release/rustyclaw ~/rustyclaw-alpha/
sudo systemctl restart rustyclaw-alpha
```

## Monitoring

### Check Service Status
```bash
sudo systemctl status rustyclaw-alpha
```

### View Logs
```bash
# Follow logs in real-time
sudo journalctl -u rustyclaw-alpha -f

# View last 100 lines
sudo journalctl -u rustyclaw-alpha -n 100

# View logs from today
sudo journalctl -u rustyclaw-alpha --since today
```

### Check Database
```bash
sqlite3 ~/rustyclaw-alpha/data.db "SELECT * FROM sessions;"
sqlite3 ~/rustyclaw-alpha/data.db "SELECT COUNT(*) FROM messages;"
```

## Troubleshooting

### Runner Not Starting
```bash
cd ~/actions-runner
sudo ./svc.sh status
sudo ./svc.sh start
```

### Build Failures
Check that Rust is installed:
```bash
rustc --version
cargo --version
```

### Service Not Running
Check logs for errors:
```bash
sudo journalctl -u rustyclaw-alpha -n 50
```

Restart the service:
```bash
sudo systemctl restart rustyclaw-alpha
```

### Database Locked
Stop the service and check for orphaned processes:
```bash
sudo systemctl stop rustyclaw-alpha
ps aux | grep rustyclaw
kill <pid> # if any orphaned processes
sudo systemctl start rustyclaw-alpha
```

## Updating Configuration

After updating the config file:
```bash
sudo systemctl restart rustyclaw-alpha
```

## Security Notes

- Keep your Telegram bot token secure in the `.env` file
- Use `allowed_users` in the config to restrict access
- Consider setting up a firewall to restrict access to the server
- Regularly update dependencies with `cargo update`
