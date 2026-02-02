# Proxmox Container Setup for RustyClaw

## Quick Setup

Your GitHub Actions runner needs a Proxmox LXC container to deploy to. This guide walks through the complete setup.

## Prerequisites

- [ ] Proxmox host with SSH access
- [ ] GitHub Actions runner token (from repo settings)
- [ ] Telegram bot token
- [ ] Container ID decided (e.g., 203, 204, 250, etc.)

## Step 1: Create LXC Container

### Option A: Proxmox Web UI

1. Go to Proxmox Web UI → Node → Local
2. Create CT
   - Hostname: `rustyclaw`
   - Container ID: `203` (or your choice)
   - OS: Debian 12
   - Root disk: 20GB
   - Memory: 2048 MB
   - Cores: 4
   - Network: DHCP or static IP
3. Start container

### Option B: Command Line

```bash
# SSH into Proxmox host
ssh root@proxmox-host

# Download Debian template (if needed)
pveam download local debian-12-standard_12.2-1_amd64.tar.zst

# Create container
pct create 203 \
  local:vztmpl/debian-12-standard_12.2-1_amd64.tar.zst \
  --hostname rustyclaw \
  --cores 4 \
  --memory 2048 \
  --swap 512 \
  --net0 name=eth0,bridge=vmbr0,ip=dhcp

# Start container
pct start 203
```

## Step 2: Setup Container Basics

### SSH into container

```bash
# From Proxmox host
pct exec 203 /bin/bash

# Or via SSH if you know the IP
# pct exec 203 ip addr show eth0  # Get IP first
ssh root@<container-ip>
```

### Install dependencies

```bash
apt-get update
apt-get upgrade -y
apt-get install -y \
  curl \
  wget \
  git \
  build-essential \
  pkg-config \
  libssl-dev \
  sqlite3 \
  jq \
  sudo

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env

# Verify
rustc --version
cargo --version
```

## Step 3: Install GitHub Actions Runner

### Get runner token

1. Go to: https://github.com/RustClaw/RustyClaw/settings/actions/runners/new
2. Select "Linux"
3. Copy the runner token (starts with `XXXXXX`)

### Inside container, install runner

```bash
# Create runner directory
cd ~
mkdir -p actions-runner
cd actions-runner

# Get latest runner version
RUNNER_VERSION=$(curl -s https://api.github.com/repos/actions/runner/releases/latest | jq -r '.tag_name' | sed 's/v//')

# Download
curl -O -L https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz
tar xzf actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz

# Configure (replace TOKEN with your token)
./config.sh \
  --url https://github.com/RustClaw/RustyClaw \
  --token PASTE_YOUR_TOKEN_HERE \
  --name "proxmox-ct203" \
  --work _work \
  --unattended \
  --replace

# Install as service
sudo ./svc.sh install

# Start service
sudo systemctl start actions.runner.RustClaw-RustyClaw.service
sudo systemctl enable actions.runner.RustClaw-RustyClaw.service

# Verify
sudo systemctl status actions.runner.RustClaw-RustyClaw.service
```

### Verify runner is registered

Go to: https://github.com/RustClaw/RustyClaw/settings/actions/runners

You should see your runner listed as "online"

## Step 4: Create RustyClaw Directories

### Inside container

```bash
# Create deployment directory
mkdir -p ~/rustyclaw-alpha/.rustyclaw
mkdir -p ~/rustyclaw-alpha/config

# Create default config
cat > ~/rustyclaw-alpha/config/config.yaml << 'EOF'
gateway:
  host: "127.0.0.1"
  port: 18789
  log_level: "info"

llm:
  provider: "ollama"
  base_url: "http://192.168.15.14:11434/v1"

  models:
    primary: "qwen2.5:32b"
    code: "deepseek-coder-v2:16b"
    fast: "qwen2.5:7b"

  cache:
    type: "ram"
    max_models: 3

channels:
  telegram:
    enabled: true
    token: "${TELEGRAM_BOT_TOKEN}"

sessions:
  scope: "per-sender"
  max_tokens: 128000

storage:
  storage_type: "sqlite"
  path: "/root/.rustyclaw/data.db"

logging:
  level: "info"
  format: "pretty"
EOF

# Verify
ls -la ~/rustyclaw-alpha/
cat ~/rustyclaw-alpha/config/config.yaml
```

## Step 5: Set Environment Variables

### Option A: Systemd service environment

Edit the systemd service:

```bash
sudo systemctl edit actions.runner.RustClaw-RustyClaw.service
```

Add under `[Service]`:

```ini
Environment="TELEGRAM_BOT_TOKEN=8228778129:AAFSzIw05Wa-yL0NwMUHnC657s-REqcjhzc"
```

Then reload:

```bash
sudo systemctl daemon-reload
sudo systemctl restart actions.runner.RustClaw-RustyClaw.service
```

### Option B: Environment file

Create `/root/.bashrc.local`:

```bash
export TELEGRAM_BOT_TOKEN="8228778129:AAFSzIw05Wa-yL0NwMUHnC657s-REqcjhzc"
export OLLAMA_HOST="http://192.168.15.14:11434"
```

## Step 6: Test Deployment

Push to main to trigger CI/CD:

```bash
# On your dev machine
cd rustyclaw
git add .
git commit -m "Test Proxmox deployment"
git push origin main
```

### Monitor deployment

Inside container:

```bash
# Watch runner logs
sudo journalctl -u actions.runner.RustClaw-RustyClaw.service -f

# Or watch the service
sudo systemctl status actions.runner.RustClaw-RustyClaw.service
```

On GitHub:

1. Go to: https://github.com/RustClaw/RustyClaw/actions
2. Click the workflow run
3. Watch "deploy-proxmox" job

## Verify Running Service

Once deployed, verify RustyClaw is running:

```bash
# Check systemd service
sudo systemctl status rustyclaw-alpha

# View logs
sudo journalctl -u rustyclaw-alpha -f

# Check port
netstat -tlnp | grep 18789

# Test API
curl http://localhost:18789/health

# Check database
ls -la ~/.rustyclaw/data.db
sqlite3 ~/.rustyclaw/data.db ".tables"
```

## Troubleshooting

### Runner not showing online

```bash
# Check runner service
sudo systemctl status actions.runner.RustClaw-RustyClaw.service

# View logs
sudo journalctl -u actions.runner.RustClaw-RustyClaw.service -n 50

# Restart
sudo systemctl restart actions.runner.RustClaw-RustyClaw.service
```

### Deployment fails

```bash
# Check CI/CD logs in GitHub Actions
https://github.com/RustClaw/RustyClaw/actions

# Common issues:
# - TELEGRAM_BOT_TOKEN not set
# - Ollama unreachable (check IP 192.168.15.14)
# - No space on disk
# - Port 18789 already in use

# Check space
df -h

# Check if Ollama is reachable
curl http://192.168.15.14:11434/api/version

# Check services
sudo systemctl status rustyclaw-alpha
sudo systemctl status actions.runner.RustClaw-RustyClaw.service
```

### Can't SSH to container

```bash
# From Proxmox host
pct exec 203 /bin/bash

# Get container IP
pct exec 203 ip addr show eth0

# Add SSH access if needed
pct exec 203 apt-get install -y openssh-server
pct exec 203 systemctl start ssh
pct exec 203 systemctl enable ssh
```

## Next Steps

1. ✅ Container created
2. ✅ Runner installed
3. ✅ RustyClaw deployed via CI/CD
4. 🔄 Test with Telegram bot
5. 🔄 Add WhatsApp integration
6. 🔄 Add Discord support

## Commands Quick Reference

```bash
# Container operations
pct list                          # List containers
pct start 203                     # Start
pct stop 203                      # Stop
pct shell 203                     # Enter shell
pct destroy 203                   # Delete

# Runner operations
sudo systemctl status actions.runner.RustClaw-RustyClaw.service
sudo systemctl restart actions.runner.RustClaw-RustyClaw.service
sudo journalctl -u actions.runner.RustClaw-RustyClaw.service -f

# RustyClaw operations
sudo systemctl status rustyclaw-alpha
sudo systemctl restart rustyclaw-alpha
sudo journalctl -u rustyclaw-alpha -f
curl http://localhost:18789/health

# Database
sqlite3 ~/.rustyclaw/data.db ".tables"
sqlite3 ~/.rustyclaw/data.db "SELECT COUNT(*) FROM messages;"
```

## Security Notes

⚠️ **Important:**
- Don't commit `.env` files to git
- Use environment variables for secrets
- Restrict container network access
- Keep Rust/dependencies updated
- Monitor logs for errors

---

Once setup is complete, every `git push origin main` will automatically:
1. Build the binary on GitHub
2. Run all tests
3. Deploy to your Proxmox container
4. Start/restart the systemd service
5. Show up in logs as "Deployment complete!"
