# LLM Cache & Hot-Swap Design

## Overview

RustyClaw's "model caching" is a **strategy layer** on top of Ollama's built-in model management. We control **when** and **how long** models stay loaded, but Ollama does the actual loading/unloading.

## Ollama's Model Lifecycle

```
Request arrives
    ↓
Is model in VRAM?
    ├─ YES → Use immediately
    └─ NO  → Is model in RAM?
              ├─ YES → Load to VRAM (~1-2 sec)
              └─ NO  → Load from SSD to VRAM (~20-30 sec)
```

## RustyClaw's Cache Strategies

### Strategy 1: RAM Caching (Aggressive)
**Best for:** Systems with 32GB+ RAM, fast responses

```yaml
llm:
  cache:
    type: "ram"
    max_models: 3
    eviction: "lru"
```

**Behavior:**
- `keep_alive: "30m"` - Models stay loaded for 30 minutes
- Ollama keeps unloaded models in system RAM
- Swap time: ~1-2 seconds (RAM → VRAM)
- Memory footprint: High (models stay in RAM)

**Use case:** User has 64GB RAM, wants fastest possible swaps

---

### Strategy 2: SSD Caching (Conservative)
**Best for:** Systems with limited RAM, acceptable latency

```yaml
llm:
  cache:
    type: "ssd"
    max_models: 1
    eviction: "immediate"
```

**Behavior:**
- `keep_alive: "2m"` - Models unload quickly
- Ollama stores models on SSD only
- Swap time: ~20-30 seconds (SSD → VRAM)
- Memory footprint: Low (only active model in RAM/VRAM)

**Use case:** User has 16GB RAM, prefers to save memory

---

### Strategy 3: No Caching (Minimal)
**Best for:** Memory-constrained systems, infrequent use

```yaml
llm:
  cache:
    type: "none"
```

**Behavior:**
- `keep_alive: "0"` - Unload immediately after response
- Always reload from SSD
- Swap time: ~20-30 seconds every time
- Memory footprint: Minimal

**Use case:** Raspberry Pi or low-RAM systems

## Implementation Components

### 1. Model Router (`src/llm/routing.rs`)

**Responsibility:** Decide which model to use for each request

```rust
pub struct ModelRouter {
    rules: Vec<RoutingRule>,
    default_model: String,
}

pub struct RoutingRule {
    pattern: Regex,           // Match against request text
    model: String,            // Model to use
    confidence: f32,          // Rule confidence
}

impl ModelRouter {
    pub fn route(&self, request: &str) -> String {
        // Check rules in order
        for rule in &self.rules {
            if rule.pattern.is_match(request) {
                return rule.model.clone();
            }
        }

        // Fallback to default
        self.default_model.clone()
    }
}
```

**Example routing rules:**
```yaml
llm:
  routing:
    default: "qwen2.5:32b"
    rules:
      - pattern: "(write|generate|implement).*code"
        model: "deepseek-coder-v2:16b"
      - pattern: "^.{0,100}$"  # Short messages
        model: "qwen2.5:7b"
```

---

### 2. Cache Manager (`src/llm/cache.rs`)

**Responsibility:** Track loaded models and manage keep_alive parameters

```rust
pub struct CacheManager {
    strategy: CacheStrategy,
    loaded_models: HashMap<String, Instant>,  // Model -> Last used
    max_models: usize,
}

pub enum CacheStrategy {
    Ram { keep_alive: Duration },
    Ssd { keep_alive: Duration },
    None,
}

impl CacheManager {
    pub fn get_keep_alive(&self) -> String {
        match &self.strategy {
            CacheStrategy::Ram { keep_alive } => {
                format!("{}s", keep_alive.as_secs())
            }
            CacheStrategy::Ssd { keep_alive } => {
                format!("{}s", keep_alive.as_secs())
            }
            CacheStrategy::None => "0".to_string(),
        }
    }

    pub fn mark_used(&mut self, model: &str) {
        self.loaded_models.insert(model.to_string(), Instant::now());

        // Evict if over limit
        if self.loaded_models.len() > self.max_models {
            self.evict_lru();
        }
    }

    fn evict_lru(&mut self) {
        // Find least recently used model
        let lru = self.loaded_models
            .iter()
            .min_by_key(|(_, last_used)| *last_used)
            .map(|(model, _)| model.clone());

        if let Some(model) = lru {
            self.loaded_models.remove(&model);
            // Note: We don't manually unload - Ollama handles this
        }
    }
}
```

---

### 3. Ollama Client Wrapper (`src/llm/client.rs`)

**Responsibility:** Send requests with appropriate keep_alive parameters

```rust
pub struct OllamaClient {
    base_url: String,
    cache_manager: Arc<RwLock<CacheManager>>,
    model_router: ModelRouter,
}

impl OllamaClient {
    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        // Route to appropriate model
        let model = self.model_router.route(&request.messages.last().unwrap().content);

        // Get keep_alive from cache strategy
        let keep_alive = {
            let cache = self.cache_manager.read().await;
            cache.get_keep_alive()
        };

        // Build Ollama request
        let ollama_request = json!({
            "model": model,
            "messages": request.messages,
            "keep_alive": keep_alive,  // ← Key parameter
            "stream": request.stream,
        });

        // Send to Ollama
        let response = self.http_client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&ollama_request)
            .send()
            .await?;

        // Mark model as used
        {
            let mut cache = self.cache_manager.write().await;
            cache.mark_used(&model);
        }

        Ok(response.json().await?)
    }
}
```

---

### 4. Preload Manager (Optional)

**Responsibility:** Warm up models before they're needed

```rust
pub struct PreloadManager {
    client: OllamaClient,
    models_to_preload: Vec<String>,
}

impl PreloadManager {
    pub async fn warmup_all(&self) -> Result<()> {
        for model in &self.models_to_preload {
            self.warmup_model(model).await?;
        }
        Ok(())
    }

    async fn warmup_model(&self, model: &str) -> Result<()> {
        // Send tiny request to load model
        let request = json!({
            "model": model,
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 1,
            "keep_alive": "30m",
        });

        self.client.http_client
            .post(format!("{}/v1/chat/completions", self.client.base_url))
            .json(&request)
            .send()
            .await?;

        info!("Preloaded model: {}", model);
        Ok(())
    }
}
```

## Configuration Examples

### Example 1: High-Performance Setup (64GB RAM)
```yaml
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
    eviction: "lru"
    keep_alive: "30m"

  routing:
    default: "primary"
    rules:
      - pattern: "(code|function|class|implement)"
        model: "code"
      - pattern: "^.{0,100}$"
        model: "fast"
```

### Example 2: Memory-Constrained Setup (16GB RAM)
```yaml
llm:
  provider: "ollama"
  base_url: "http://192.168.15.14:11434/v1"

  models:
    primary: "qwen2.5:7b"  # Smaller primary model

  cache:
    type: "ssd"
    max_models: 1
    keep_alive: "2m"

  routing:
    default: "primary"
```

## Performance Expectations

| Cache Type | Swap Time | RAM Usage | Best For |
|------------|-----------|-----------|----------|
| RAM        | 1-2 sec   | High (20-40GB) | Desktop with 64GB+ RAM |
| SSD        | 20-30 sec | Low (4-8GB) | Laptop with 16GB RAM |
| None       | 20-30 sec | Minimal (2-4GB) | Raspberry Pi, edge devices |

## API Transparency

**Important:** From the user's perspective (Telegram, Discord, etc.), model swapping is **transparent**:

```
User: "Write a function to sort an array"
    ↓
Router detects "code" task
    ↓
Switches to deepseek-coder-v2:16b (if not loaded)
    ↓
Returns response

User has no idea a swap happened!
```

## Monitoring & Metrics

Optional: Track model usage and swap performance

```rust
pub struct CacheMetrics {
    swaps: HashMap<String, u64>,        // Model -> Swap count
    swap_times: HashMap<String, f64>,   // Model -> Avg swap time
    cache_hits: u64,
    cache_misses: u64,
}
```

Expose via API:
```
GET /api/llm/stats
{
  "cache_strategy": "ram",
  "loaded_models": ["qwen2.5:32b", "deepseek-coder-v2:16b"],
  "cache_hit_rate": 0.85,
  "avg_swap_time_ms": 1200
}
```

## Summary

**What Ollama provides:**
- Automatic model loading/unloading
- VRAM management
- Model storage (SSD)
- keep_alive parameter support

**What RustyClaw builds:**
- Intelligent model routing
- Cache strategy abstraction (RAM/SSD/None)
- keep_alive parameter management
- Optional preloading
- Usage tracking

**Key insight:** We're building a **policy engine**, not a cache implementation. Ollama does the heavy lifting.
