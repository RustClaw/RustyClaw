# RustyClaw Deployment Guide

## Quick Start (Docker Compose)

### Prerequisites
- Docker & Docker Compose installed
- (Optional) NVIDIA GPU with drivers for LLM inference

### 1. Clone and Configure

```bash
git clone https://github.com/your-org/rustyclaw
cd rustyclaw

# Copy environment template
cp .env.example .env

# Edit with your bot tokens
nano .env
```

### 2. Directory Structure

```
rustyclaw/
├── docker-compose.yml
├── .env                    # Your secrets (not in git)
├── data/                   # ← Created automatically
│   ├── config.yaml         # RustyClaw configuration
│   ├── data.db            # ← SQLite database (persistent)
│   ├── workspace/          # Agent workspace
│   └── logs/               # Application logs
└── ...
```

### 3. Start Services

```bash
# Start all services
docker-compose up -d

# Check status
docker-compose ps

# View logs
docker-compose logs -f rustyclaw
```

### 4. Pull LLM Models

```bash
# If using dockerized Ollama
docker exec ollama ollama pull qwen2.5:32b
docker exec ollama ollama pull deepseek-coder-v2:16b
docker exec ollama ollama pull qwen2.5:7b

# Or if using external Ollama VM (192.168.15.14)
ssh ollama@192.168.15.14 "ollama pull qwen2.5:32b"
```

### 5. Verify Deployment

```bash
# Check health
curl http://localhost:18789/health

# Check LLM connection
curl http://localhost:11434/api/version
```

---

## Storage: SQLite in Docker

### How It Works

```
Host Machine                Docker Container
───────────                ────────────────
./data/                →  /root/.rustyclaw/
  ├── config.yaml      →    ├── config.yaml
  ├── data.db          →    ├── data.db  ← SQLite database
  ├── workspace/       →    ├── workspace/
  └── logs/            →    └── logs/
```

**Key Points:**
- ✅ **Persistent**: SQLite file stored on host in `./data/data.db`
- ✅ **Survives restarts**: Data persists when container restarts
- ✅ **Easy backup**: `cp -r ./data ./backup`
- ✅ **No external DB**: Self-contained, no PostgreSQL/MySQL needed
- ✅ **Portable**: Move `./data` directory to migrate

### Database Location

| Environment | Database Path |
|-------------|---------------|
| **Docker (default)** | `/root/.rustyclaw/data.db` (inside container)<br>`./data/data.db` (on host) |
| **Binary install** | `~/.rustyclaw/data.db` |
| **Custom** | Set in `config.yaml`: `storage.path` |

---

## Deployment Scenarios

### Scenario 1: All-in-One Docker (Simplest)

**Use when:** Single server with GPU

```yaml
# docker-compose.yml
services:
  rustyclaw:    # Gateway
  ollama:       # LLM (uses GPU)
```

```bash
docker-compose up -d
# Everything runs on one machine
```

**Pros:**
- Easiest setup
- One command deployment
- All services managed together

**Cons:**
- Requires GPU on same server as gateway
- Higher resource usage on single machine

---

### Scenario 2: Split Deployment (Recommended for Production)

**Use when:** Separate GPU server (like your Proxmox setup)

```yaml
# docker-compose.yml on App Server
services:
  rustyclaw:
    environment:
      - OLLAMA_HOST=http://192.168.15.14:11434  # External LLM VM
```

```bash
# App Server
docker-compose up -d

# LLM Server (Proxmox VM - already running)
ssh ollama@192.168.15.14
ollama serve  # Already running
```

**Architecture:**
```
┌─────────────────┐          ┌──────────────────┐
│  App Server     │          │  LLM VM          │
│  (Docker)       │  HTTP    │  (192.168.15.14) │
│                 │ ──────▶  │                  │
│  - RustyClaw    │          │  - Ollama        │
│  - SQLite DB    │          │  - 2x RTX 3060   │
│  - Telegram     │          │  - qwen2.5:32b   │
└─────────────────┘          │  - deepseek-v2   │
                             └──────────────────┘
```

**Pros:**
- Dedicated GPU server
- Gateway can run on low-power machine
- Scale independently
- Better resource utilization

**Cons:**
- Network latency (minimal: ~1-5ms on LAN)
- Two servers to manage

---

## Configuration

### Minimal config.yaml

```yaml
# data/config.yaml

llm:
  provider: "ollama"
  base_url: "http://ollama:11434/v1"  # or http://192.168.15.14:11434/v1

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
  path: "/root/.rustyclaw/data.db"  # In container

sessions:
  scope: "per-sender"
  max_tokens: 128000
```

---

## Backup & Recovery

### Backup

```bash
# Stop services
docker-compose stop

# Backup data directory (includes SQLite DB)
tar -czf rustyclaw-backup-$(date +%Y%m%d).tar.gz ./data

# Restart
docker-compose start
```

### Recovery

```bash
# Stop services
docker-compose stop

# Restore backup
tar -xzf rustyclaw-backup-20260201.tar.gz

# Restart
docker-compose start
```

### Automated Daily Backup

```bash
# Add to crontab
0 3 * * * cd /path/to/rustyclaw && tar -czf backups/backup-$(date +\%Y\%m\%d).tar.gz ./data
```

---

## Monitoring

### Logs

```bash
# View all logs
docker-compose logs -f

# RustyClaw only
docker-compose logs -f rustyclaw

# Last 100 lines
docker-compose logs --tail=100 rustyclaw
```

### Database Size

```bash
# Check SQLite database size
docker exec rustyclaw du -h /root/.rustyclaw/data.db

# Or on host
du -h ./data/data.db
```

### Resource Usage

```bash
# Container stats
docker stats rustyclaw ollama

# Disk usage
docker system df
```

---

## Scaling & Performance

### SQLite Limits

| Metric | Limit | Notes |
|--------|-------|-------|
| **Database size** | ~280 TB | More than enough |
| **Concurrent writes** | ~1000/sec | Fine for single user/small team |
| **Concurrent reads** | Unlimited | Multiple readers OK |
| **Sessions** | Millions | No practical limit |

**When to upgrade to PostgreSQL:**
- Multiple RustyClaw instances (load balancing)
- >100 concurrent users
- Need advanced analytics/reporting
- Multi-region deployment

### Migration to PostgreSQL (Future)

```yaml
# config.yaml
storage:
  storage_type: "postgres"
  url: "postgres://user:pass@db:5432/rustyclaw"
```

```bash
# Run migration tool (TBD)
rustyclaw migrate sqlite-to-postgres
```

---

## Troubleshooting

### Container won't start

```bash
# Check logs
docker-compose logs rustyclaw

# Common issues:
# 1. Port already in use
sudo lsof -i :18789

# 2. Volume permission issues
sudo chown -R 1000:1000 ./data
```

### SQLite database locked

```bash
# Check for stale connections
docker exec rustyclaw fuser /root/.rustyclaw/data.db

# Restart gateway
docker-compose restart rustyclaw
```

### Can't connect to LLM

```bash
# Test Ollama endpoint
curl http://ollama:11434/api/version

# Or external VM
curl http://192.168.15.14:11434/api/version

# Check network
docker-compose exec rustyclaw ping ollama
```

---

## Updates

### Update RustyClaw

```bash
# Pull latest image
docker-compose pull rustyclaw

# Recreate container (preserves data)
docker-compose up -d

# View logs
docker-compose logs -f rustyclaw
```

### Update Models

```bash
# Update to latest model version
docker exec ollama ollama pull qwen2.5:32b

# Check available updates
docker exec ollama ollama list
```

---

## Production Checklist

- [ ] Set strong bot tokens in `.env`
- [ ] Configure allowed users (if needed)
- [ ] Set up automated backups
- [ ] Configure log rotation
- [ ] Set up monitoring/alerts
- [ ] Test backup restoration
- [ ] Document custom configuration
- [ ] Set up reverse proxy (nginx) for HTTPS
- [ ] Configure firewall rules

---

## Next Steps

1. **Test basic functionality**
   ```bash
   # Send message to your Telegram bot
   # Verify it responds
   ```

2. **Monitor for a few days**
   ```bash
   # Check logs regularly
   docker-compose logs -f rustyclaw
   ```

3. **Tune configuration**
   - Adjust model routing rules
   - Configure context limits
   - Set up cron jobs

4. **Add more channels**
   - Discord
   - WhatsApp
   - Web API
