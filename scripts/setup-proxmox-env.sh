#!/bin/bash
# Setup RustyClaw environment on Proxmox server
# Run this AFTER setting up the GitHub runner

set -e

echo "🔧 Setting up RustyClaw environment on Proxmox"
echo ""

# Create directories
echo "📁 Creating directories..."
mkdir -p ~/rustyclaw-alpha/config
mkdir -p ~/rustyclaw-alpha/logs

# Check if Ollama is installed
if ! command -v ollama &> /dev/null; then
    echo "🤖 Installing Ollama..."
    curl -fsSL https://ollama.com/install.sh | sh

    # Start Ollama service
    sudo systemctl enable ollama
    sudo systemctl start ollama

    echo "⏳ Waiting for Ollama to start..."
    sleep 5

    echo "📥 Pulling model..."
    ollama pull qwen2.5:7b
else
    echo "✅ Ollama already installed"
fi

# Create config file
echo "📝 Creating config file..."
cat > ~/rustyclaw-alpha/config/config.yaml <<EOF
llm:
  base_url: "http://localhost:11434/v1"
  models:
    primary: "qwen2.5:7b"

channels:
  telegram:
    enabled: true
    token: "\${TELEGRAM_BOT_TOKEN}"
    allowed_users: []  # Add your Telegram user ID here for security

sessions:
  scope: "per-sender"
  max_tokens: 128000

storage:
  storage_type: "sqlite"
  path: "$HOME/rustyclaw-alpha/data.db"

logging:
  level: "info"
  format: "pretty"
EOF

# Create .env file template
cat > ~/rustyclaw-alpha/.env.example <<EOF
# Copy this to .env and fill in your values
TELEGRAM_BOT_TOKEN=your_bot_token_here
EOF

echo ""
echo "✅ Environment setup complete!"
echo ""
echo "Next steps:"
echo "1. Create your Telegram bot:"
echo "   - Message @BotFather on Telegram"
echo "   - Send: /newbot"
echo "   - Follow the instructions"
echo "   - Copy the bot token"
echo ""
echo "2. Add your bot token to the environment:"
echo "   cd ~/rustyclaw-alpha"
echo "   cp .env.example .env"
echo "   nano .env  # Add your TELEGRAM_BOT_TOKEN"
echo ""
echo "3. (Optional) Restrict access to your user ID:"
echo "   - Message your bot on Telegram"
echo "   - Check the logs after deployment to see your user ID"
echo "   - Add it to allowed_users in config/config.yaml"
echo ""
echo "4. Push to main branch to trigger deployment:"
echo "   git push origin main"
echo ""
echo "5. Monitor deployment:"
echo "   sudo journalctl -u rustyclaw-alpha -f"
echo ""
