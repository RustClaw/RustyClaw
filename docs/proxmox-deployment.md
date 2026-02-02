# RustyClaw Proxmox Deployment Guide

## Overview

RustyClaw deploys directly to Proxmox container `ct202` as a systemd service. SQLite database is stored locally on the container.

## Architecture

```
┌─────────────────────────────────────────┐
│         Proxmox Host                    │
│                                         │
│  ┌─────────────────────────────────┐   │
│  │  Container ct202                │   │
│  │                                 │   │
│  │  RustyClaw Gateway              │   │
│  │  ├─ Binary: /root/rustyclaw    │   │
│  │  ├─ Config: ~/.rustyclaw/       │   │
│  │  └─ SQLite: ~/.rustyclaw/data.db│   │
│  │                                 │   │
│  │  systemd service: rustyclaw-alpha   │
│  └─────────────────────────────────┘   │
│           ↓ HTTP (port 18789)           │
│  ┌─────────────────────────────────┐   │
│  │  External Ollama VM             │   │
│  │  (192.168.15.14:11434)          │   │
│  │  ├─ qwen2.5:32b                 │   │
│  │  ├─ deepseek-coder-v2:16b       │   │
│  │  └─ qwen2.5:7b                  │   │
│  └─────────────────────────────────┘   │
└─────────────────────────────────────────┘
```

## Quick Deployment

### 1. Trigger CI/CD

Push to `main` branch:

```bash
git add .
git commit -m "Update RustyClaw configuration"
git push origin main
```

The GitHub Actions workflow automatically:
- Builds the release binary
- Runs tests on Ubuntu
- Deploys to ct202 via self-hosted runner
- Starts the systemd service

### 2. Monitor Deployment

```bash
# SSH into ct202
ssh root@proxmox-host -t "lxc-attach -n ct202"

# View systemd status
systemctl status rustyclaw-alpha

# View logs in real-time
journalctl -u rustyclaw-alpha -f

# View last 100 lines
journalctl -u rustyclaw-alpha -n 100
```

### 3. Manual Restart (if needed)

```bash
systemctl restart rustyclaw-alpha
```

## Configuration

Configuration file: `~/.rustyclaw/config.yaml`

```yaml
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

storage:
  storage_type: "sqlite"
  path: "~/.rustyclaw/data.db"

sessions:
  scope: "per-sender"
  max_tokens: 128000
```

## Database

### Location

```
Container ct202:
~/.rustyclaw/data.db         ← SQLite database file
~/.rustyclaw/               ← Configuration directory
├── config.yaml
├── workspace/
└── logs/
```

### Schema

Automatically created on first run via SQLite migrations:
- `sessions` - Conversation sessions per user/channel
- `messages` - Message history with model tracking
- `cron_jobs` - Scheduled tasks (future)
- `plugin_state` - Plugin data (future)

### Backup

```bash
# SSH into ct202
ssh root@proxmox-host -t "lxc-attach -n ct202"

# Create backup
tar -czf /var/backups/rustyclaw-$(date +%Y%m%d-%H%M%S).tar.gz ~/.rustyclaw/

# Verify backup
tar -tzf /var/backups/rustyclaw-*.tar.gz | head -20
```

### Restore

```bash
# Stop service
systemctl stop rustyclaw-alpha

# Restore backup
tar -xzf /var/backups/rustyclaw-20260201-120000.tar.gz -C ~/

# Start service
systemctl start rustyclaw-alpha
```

## Environment Variables

Set in `/etc/systemd/system/rustyclaw-alpha.service` or via:

```bash
sudo systemctl set-environment TELEGRAM_BOT_TOKEN="your_token_here"
```

Or edit the service file:

```bash
sudo systemctl edit rustyclaw-alpha
```

Add:
```ini
Environment="TELEGRAM_BOT_TOKEN=your_token_here"
```

## Port Mapping

RustyClaw listens on `127.0.0.1:18789` inside ct202.

To access from outside Proxmox host:
1. Use SSH tunnel: `ssh root@proxmox -L 18789:127.0.0.1:18789`
2. Or expose to network in config and use firewall

## Logs

### Real-time Logs

```bash
journalctl -u rustyclaw-alpha -f
```

### Logs with Filters

```bash
# Last hour
journalctl -u rustyclaw-alpha --since "1 hour ago"

# Errors only
journalctl -u rustyclaw-alpha PRIORITY=err

# Specific service
journalctl -u rustyclaw-alpha -n 500
```

### Persistent Logs (optional)

Store logs to file:

```bash
sudo tee -a /etc/systemd/system/rustyclaw-alpha.service.d/override.conf > /dev/null <<EOF
[Service]
StandardOutput=journal
StandardError=journal
SyslogIdentifier=rustyclaw
EOF

sudo systemctl daemon-reload
sudo systemctl restart rustyclaw-alpha
```

## Troubleshooting

### Service won't start

```bash
# Check status
systemctl status rustyclaw-alpha

# View last 50 lines of logs
journalctl -u rustyclaw-alpha -n 50

# Check configuration
cat ~/.rustyclaw/config.yaml

# Verify Ollama connection
curl http://192.168.15.14:11434/api/version
```

### Database locked

```bash
# Restart service
systemctl restart rustyclaw-alpha

# Check if process is running
ps aux | grep rustyclaw
```

### Can't connect to Ollama

```bash
# Verify Ollama is running on 192.168.15.14
curl http://192.168.15.14:11434/api/tags

# Check network connectivity
ping 192.168.15.14

# Verify config has correct URL
grep "base_url" ~/.rustyclaw/config.yaml
```

## Updates

### Deploy New Version

```bash
# 1. Commit changes
git add .
git commit -m "Feature: Add X"
git push origin main

# 2. Wait for CI/CD to deploy (watch Actions tab)
# 3. Verify in Proxmox:
journalctl -u rustyclaw-alpha -f
```

### Rollback

```bash
# If deployment breaks, revert last commit
git revert HEAD
git push origin main

# CI/CD will automatically deploy previous version
```

## Monitoring

### Health Check

```bash
# Check if service is running
systemctl is-active rustyclaw-alpha

# Check port is listening
ss -tlnp | grep 18789

# Test API
curl http://localhost:18789/health || echo "Service not responding"
```

### Resource Usage

```bash
# Container resource limits
lxc info ct202 | grep -A 20 resources

# Current process usage
ps aux | grep rustyclaw

# Disk usage
du -sh ~/.rustyclaw/
```

## Security

### Telegram Bot Token

Store in systemd service environment:

```bash
sudo systemctl edit rustyclaw-alpha
```

Add:
```ini
Environment="TELEGRAM_BOT_TOKEN=your_secret_token"
```

Never commit tokens to git!

### Firewall Rules

If exposing to network:

```bash
# Allow only trusted IPs
iptables -A INPUT -p tcp --dport 18789 -s YOUR_IP -j ACCEPT
iptables -A INPUT -p tcp --dport 18789 -j DROP
```

## Production Checklist

- [ ] Telegram bot token configured in systemd service
- [ ] Ollama VM (192.168.15.14) is reachable
- [ ] SQLite database backed up
- [ ] Systemd service enabled and starting on boot
- [ ] Logs being monitored
- [ ] Firewall rules configured
- [ ] Database migrations run successfully
- [ ] Test conversation works end-to-end

## Additional Resources

- Systemd logs: `journalctl -u rustyclaw-alpha`
- Configuration file: `~/.rustyclaw/config.yaml`
- Database: `~/.rustyclaw/data.db` (SQLite)
- Binary: `/root/rustyclaw-alpha/rustyclaw`
- Service file: `/etc/systemd/system/rustyclaw-alpha.service`
