#!/bin/bash

# RustyClaw Proxmox Container Setup Script
# This script creates and configures an LXC container for RustyClaw with GitHub Actions runner

set -e

# Configuration
CONTAINER_ID=${1:-203}  # Change to your desired container ID (e.g., 203, 204, etc.)
CONTAINER_NAME="rustyclaw"
PROXMOX_HOST="${2:-proxmox}"
RUNNER_TOKEN="${3:-}"

echo "================================"
echo "RustyClaw Proxmox Container Setup"
echo "================================"
echo ""
echo "Container ID: $CONTAINER_ID"
echo "Container Name: $CONTAINER_NAME"
echo ""

if [ -z "$RUNNER_TOKEN" ]; then
    echo "ERROR: GitHub Actions runner token required!"
    echo "Usage: $0 <container_id> <proxmox_host> <runner_token>"
    echo ""
    echo "To get the runner token:"
    echo "1. Go to https://github.com/RustClaw/RustyClaw/settings/actions/runners/new"
    echo "2. Select 'Linux' and copy the registration token"
    echo "3. Run: $0 $CONTAINER_ID $PROXMOX_HOST <token>"
    exit 1
fi

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Step 1: Create LXC container
echo -e "${YELLOW}Step 1: Creating LXC container...${NC}"
cat > /tmp/container_setup.sh << 'CONTAINER_SETUP'
#!/bin/bash

# This script runs inside the container after creation

echo "Updating system packages..."
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
    jq

echo "Installing Rust..."
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env

echo "Creating rustyclaw directories..."
mkdir -p ~/.rustyclaw/workspace
mkdir -p ~/rustyclaw-alpha

echo "Installation complete!"
CONTAINER_SETUP

# Create the container (adjust as needed for your Proxmox setup)
echo "Note: You may need to run this manually in Proxmox or via pveam/lxc commands:"
echo ""
echo "pveam download local debian-12-standard_12.2-1_amd64.tar.zst"
echo "pct create $CONTAINER_ID local:vztmpl/debian-12-standard_12.2-1_amd64.tar.zst --hostname $CONTAINER_NAME --cores 4 --memory 2048 --swap 512 --net0 name=eth0,bridge=vmbr0,ip=dhcp"
echo ""
echo "Then run the setup inside the container:"
echo "pct exec $CONTAINER_ID /bin/bash /tmp/container_setup.sh"
echo ""
read -p "Continue? (y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    exit 1
fi

# Step 2: Install GitHub Actions Runner
echo -e "${YELLOW}Step 2: Installing GitHub Actions Runner...${NC}"
cat > /tmp/install_runner.sh << 'RUNNER_SETUP'
#!/bin/bash

RUNNER_TOKEN="$1"
CONTAINER_ID="$2"

# Download and setup runner
cd ~
mkdir -p actions-runner
cd actions-runner

# Get latest runner version
RUNNER_VERSION=$(curl -s https://api.github.com/repos/actions/runner/releases/latest | jq -r '.tag_name' | sed 's/v//')

echo "Installing GitHub Actions Runner v${RUNNER_VERSION}..."
curl -O -L https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz
tar xzf actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz

# Configure runner
echo "Configuring runner for RustClaw/RustyClaw..."
./config.sh \
    --url https://github.com/RustClaw/RustyClaw \
    --token "$RUNNER_TOKEN" \
    --name "proxmox-ct${CONTAINER_ID}" \
    --work _work \
    --unattended \
    --replace \
    --runnergroup Default \
    --labels linux,rustyclaw,proxmox

# Install as service
echo "Installing runner as systemd service..."
sudo ./svc.sh install

# Start service
echo "Starting runner service..."
sudo systemctl start actions.runner.RustClaw-RustyClaw.service
sudo systemctl enable actions.runner.RustClaw-RustyClaw.service

echo "Runner installed and started!"
echo "Check status: sudo systemctl status actions.runner.RustClaw-RustyClaw.service"
RUNNER_SETUP

# Execute runner setup
chmod +x /tmp/install_runner.sh
echo "Execute this on the container:"
echo "bash /tmp/install_runner.sh '$RUNNER_TOKEN' $CONTAINER_ID"
echo ""

# Step 3: Create deployment directory
echo -e "${YELLOW}Step 3: Creating deployment directories...${NC}"
cat > /tmp/setup_dirs.sh << 'SETUP_DIRS'
#!/bin/bash

echo "Creating deployment directories..."

# Create directories
mkdir -p ~/rustyclaw-alpha/.rustyclaw
mkdir -p ~/rustyclaw-alpha/config

# Create default config
cat > ~/rustyclaw-alpha/config/config.yaml << 'CONFIG'
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
CONFIG

echo "Deployment directories created!"
echo "Location: ~/rustyclaw-alpha"
ls -la ~/rustyclaw-alpha/
SETUP_DIRS

chmod +x /tmp/setup_dirs.sh

echo ""
echo -e "${GREEN}Setup complete!${NC}"
echo ""
echo "Next steps:"
echo "1. Create container: pct create $CONTAINER_ID ..."
echo "2. Install runner: bash /tmp/install_runner.sh '$RUNNER_TOKEN' $CONTAINER_ID"
echo "3. Setup directories: bash /tmp/setup_dirs.sh"
echo "4. Set environment: export TELEGRAM_BOT_TOKEN='...'"
echo "5. Push code: git push origin main (CI/CD will auto-deploy)"
echo ""
echo "Monitor runner at:"
echo "https://github.com/RustClaw/RustyClaw/settings/actions/runners"
echo ""
echo "View runner logs:"
echo "pct exec $CONTAINER_ID journalctl -u actions.runner.RustClaw-RustyClaw.service -f"
