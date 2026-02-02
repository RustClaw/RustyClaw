# RustyClaw Deployment Guide

## Deployment Options

Choose your deployment method:

### **Proxmox (Recommended for RustyClaw) ⭐**

Direct deployment to Proxmox container `ct202` with systemd service.

- ✅ Automated CI/CD via GitHub Actions self-hosted runner
- ✅ SQLite database on container filesystem
- ✅ Direct binary execution (no container overhead)
- ✅ Easy logging via `journalctl`
- ✅ Simple restarts and updates

**Setup:** See [proxmox-deployment.md](./proxmox-deployment.md)

---

### **Docker Compose (Self-Hosted Server)**

All-in-one deployment with containerized services.

- ✅ Services run in containers
- ✅ Easy to move between servers
- ✅ Docker Compose orchestration
- ⚠️ Higher resource overhead

**Setup:** See [deployment-guide.md](./deployment-guide.md)

---

### **Binary Installation (Any Linux)**

Install as standalone binary on your machine.

```bash
# Download or build binary
cargo build --release

# Run
./target/release/rustyclaw --config ~/.rustyclaw/config.yaml serve
```

---

## Quick Start (Proxmox)

### 1. Prerequisites

- ✅ Proxmox container `ct202` running
- ✅ Self-hosted GitHub Actions runner installed in ct202
- ✅ Ollama VM running at `192.168.15.14:11434`

### 2. Deploy

```bash
# Push to main branch
git add .
git commit -m "Deploy to Proxmox"
git push origin main

# Watch CI/CD
# - Build runs on GitHub
# - Deploy runs on self-hosted runner in ct202
# - Service restarts automatically
```

### 3. Verify

```bash
# SSH into Proxmox
ssh root@proxmox-host -t "lxc-attach -n ct202"

# Check service status
systemctl status rustyclaw-alpha

# View logs
journalctl -u rustyclaw-alpha -f
```

---

## Storage

### SQLite (Default)

Database file stored in container/system:

```
~/.rustyclaw/data.db
```

**Advantages:**
- No external database needed
- Easy backup (copy file)
- Portable
- Fast for single-user/small team

**When to upgrade:**
- Multiple RustyClaw instances
- >100 concurrent users
- Multi-region deployment

---

## Architecture Comparison

| Feature | Proxmox | Docker Compose | Binary |
|---------|---------|----------------|--------|
| **Setup Complexity** | ⭐ Easy | ⭐⭐ Medium | ⭐ Easy |
| **Resource Usage** | ⭐ Minimal | ⭐⭐ Higher | ⭐ Minimal |
| **Portability** | ⭐⭐ Medium | ⭐⭐⭐ High | ⭐⭐ Medium |
| **Logging** | journalctl | docker logs | logs file |
| **CI/CD** | ✅ GitHub Actions | Manual | Manual |
| **Recommended** | ✅ Yes | For servers | Simple setups |

---

## Configuration

All deployments use the same config file:

```yaml
llm:
  provider: "ollama"
  base_url: "http://192.168.15.14:11434/v1"

  models:
    primary: "qwen2.5:32b"
    code: "deepseek-coder-v2:16b"
    fast: "qwen2.5:7b"

channels:
  telegram:
    enabled: true
    token: "${TELEGRAM_BOT_TOKEN}"

storage:
  storage_type: "sqlite"
  path: "~/.rustyclaw/data.db"
```

---

## Database Backups

### Proxmox

```bash
# SSH into ct202
ssh root@proxmox -t "lxc-attach -n ct202"

# Backup
tar -czf backup-$(date +%Y%m%d).tar.gz ~/.rustyclaw/

# Store safely
mv backup-*.tar.gz /var/backups/
```

### Docker Compose

```bash
# Backup from host
tar -czf backup-$(date +%Y%m%d).tar.gz ./data/
```

---

## Monitoring

### Proxmox

```bash
# SSH into ct202
ssh root@proxmox -t "lxc-attach -n ct202"

# Real-time logs
journalctl -u rustyclaw-alpha -f

# Check status
systemctl status rustyclaw-alpha

# Resource usage
ps aux | grep rustyclaw
```

### Docker Compose

```bash
# View logs
docker-compose logs -f rustyclaw

# Check status
docker-compose ps

# Resource usage
docker stats rustyclaw
```

---

## Troubleshooting

### Proxmox

```bash
# SSH into ct202
ssh root@proxmox -t "lxc-attach -n ct202"

# Check service
systemctl status rustyclaw-alpha

# Check logs for errors
journalctl -u rustyclaw-alpha -p err

# Restart service
systemctl restart rustyclaw-alpha

# Check Ollama connection
curl http://192.168.15.14:11434/api/version
```

### Docker Compose

```bash
# Check logs
docker-compose logs rustyclaw

# Restart container
docker-compose restart rustyclaw

# Check network
docker-compose exec rustyclaw ping 192.168.15.14
```

---

## Next Steps

1. **Choose deployment method** → Follow corresponding guide
2. **Configure application** → Edit `config.yaml`
3. **Test bot** → Send message to Telegram bot
4. **Monitor logs** → Watch for errors
5. **Tune settings** → Adjust model routing, context limits

---

## Documentation Links

- [Proxmox Deployment](./proxmox-deployment.md) - Direct to ct202
- [Docker Deployment](./deployment-guide.md) - Docker Compose
- [Architecture](../rustyclaw.md) - System design
- [Implementation Plan](./implementation-plan-llm.md) - Development phases
- [Progress Report](./progress-llm-integration.md) - Current status
