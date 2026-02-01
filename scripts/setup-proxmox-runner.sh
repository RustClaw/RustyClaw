#!/bin/bash
# Setup script for GitHub Actions self-hosted runner on Proxmox
# Run this script on your Proxmox server

set -e

echo "🚀 Setting up GitHub Actions Runner for RustyClaw"
echo ""

# Check if running as root
if [ "$EUID" -eq 0 ]; then
   echo "❌ Please do not run as root. Run as the user that will execute the runner."
   exit 1
fi

# Install dependencies
echo "📦 Installing dependencies..."
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev curl git

# Install Rust if not already installed
if ! command -v cargo &> /dev/null; then
    echo "🦀 Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo "✅ Rust already installed"
fi

# Create runner directory
echo "📁 Creating runner directory..."
mkdir -p ~/actions-runner && cd ~/actions-runner

# Download latest runner
echo "⬇️  Downloading GitHub Actions runner..."
RUNNER_VERSION="2.331.0"
curl -o actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz -L \
  https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz

# Optional: Validate the hash
echo "5fcc01bd546ba5c3f1291c2803658ebd3cedb3836489eda3be357d41bfcf28a7  actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz" | shasum -a 256 -c

# Extract
echo "📦 Extracting runner..."
tar xzf ./actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz

echo ""
echo "✅ Runner downloaded and extracted"
echo ""
echo "Next steps:"
echo ""
echo "1. Configure the runner with your token:"
echo "   cd ~/actions-runner"
echo "   ./config.sh --url https://github.com/RustClaw --token YOUR_TOKEN_HERE"
echo ""
echo "2. Install and start the service:"
echo "   sudo ./svc.sh install"
echo "   sudo ./svc.sh start"
echo ""
echo "3. Verify the runner is running:"
echo "   sudo ./svc.sh status"
echo ""
echo "Note: Get your token from https://github.com/RustClaw/settings/actions/runners/new"
echo ""
