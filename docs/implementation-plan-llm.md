# LLM Integration Implementation Plan

## Overview

Implement hot-swapping LLM integration with Ollama, including model routing, context management, and cache strategies.

## Phase 1: Core LLM Client (Current Phase)

### Components to Build

```
src/llm/
├── mod.rs              # Module exports
├── client.rs           # Ollama HTTP client
├── models.rs           # Model definitions & config
├── routing.rs          # Model routing logic
└── cache.rs            # Cache strategy manager
```

### 1. Model Definitions (`src/llm/models.rs`)

**Purpose:** Define data structures for LLM requests/responses

```rust
pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub stream: bool,
    pub max_tokens: Option<usize>,
}

pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub usage: TokenUsage,
}

pub struct Message {
    pub role: Role,
    pub content: String,
}

pub enum Role {
    System,
    User,
    Assistant,
}

pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}
```

### 2. Cache Strategy (`src/llm/cache.rs`)

**Purpose:** Manage keep_alive parameters based on cache strategy

```rust
#[derive(Debug, Clone)]
pub enum CacheStrategy {
    Ram { keep_alive: Duration },
    Ssd { keep_alive: Duration },
    None,
}

impl CacheStrategy {
    pub fn from_config(config: &LlmConfig) -> Self {
        match config.cache.cache_type.as_str() {
            "ram" => CacheStrategy::Ram {
                keep_alive: Duration::from_secs(30 * 60), // 30 minutes
            },
            "ssd" => CacheStrategy::Ssd {
                keep_alive: Duration::from_secs(2 * 60), // 2 minutes
            },
            _ => CacheStrategy::None,
        }
    }

    pub fn keep_alive_string(&self) -> String {
        match self {
            CacheStrategy::Ram { keep_alive } => format!("{}m", keep_alive.as_secs() / 60),
            CacheStrategy::Ssd { keep_alive } => format!("{}m", keep_alive.as_secs() / 60),
            CacheStrategy::None => "0".to_string(),
        }
    }
}

pub struct CacheManager {
    strategy: CacheStrategy,
    loaded_models: HashMap<String, Instant>,
    max_models: usize,
}

impl CacheManager {
    pub fn mark_used(&mut self, model: &str) {
        self.loaded_models.insert(model.to_string(), Instant::now());
        if self.loaded_models.len() > self.max_models {
            self.evict_lru();
        }
    }

    fn evict_lru(&mut self) {
        // Find least recently used model
        if let Some((lru_model, _)) = self.loaded_models
            .iter()
            .min_by_key(|(_, last_used)| *last_used)
        {
            let model = lru_model.clone();
            self.loaded_models.remove(&model);
            tracing::debug!("Evicted LRU model from cache tracking: {}", model);
        }
    }
}
```

### 3. Model Router (`src/llm/routing.rs`)

**Purpose:** Intelligent model selection based on request content

```rust
pub struct ModelRouter {
    default_model: String,
    rules: Vec<RoutingRule>,
}

pub struct RoutingRule {
    pattern: Regex,
    model: String,
}

impl ModelRouter {
    pub fn new(config: &LlmConfig) -> Result<Self> {
        let mut rules = Vec::new();

        // Parse routing rules from config
        if let Some(routing) = &config.routing {
            for rule in &routing.rules {
                rules.push(RoutingRule {
                    pattern: Regex::new(&rule.pattern)?,
                    model: rule.model.clone(),
                });
            }
        }

        Ok(Self {
            default_model: config.models.primary.clone(),
            rules,
        })
    }

    pub fn route(&self, message: &str) -> &str {
        // Check rules in order
        for rule in &self.rules {
            if rule.pattern.is_match(message) {
                tracing::debug!(
                    "Routing to model '{}' based on pattern '{}'",
                    rule.model,
                    rule.pattern.as_str()
                );
                return &rule.model;
            }
        }

        // Fallback to default
        tracing::debug!("Routing to default model '{}'", self.default_model);
        &self.default_model
    }
}
```

### 4. Ollama Client (`src/llm/client.rs`)

**Purpose:** HTTP client for Ollama API with hot-swap support

```rust
pub struct OllamaClient {
    base_url: String,
    http_client: reqwest::Client,
    cache_manager: Arc<Mutex<CacheManager>>,
    router: ModelRouter,
}

impl OllamaClient {
    pub fn new(config: &LlmConfig) -> Result<Self> {
        let cache_strategy = CacheStrategy::from_config(config);
        let cache_manager = CacheManager {
            strategy: cache_strategy,
            loaded_models: HashMap::new(),
            max_models: config.cache.max_models,
        };

        Ok(Self {
            base_url: config.base_url.clone(),
            http_client: reqwest::Client::new(),
            cache_manager: Arc::new(Mutex::new(cache_manager)),
            router: ModelRouter::new(config)?,
        })
    }

    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        // Route to appropriate model
        let last_message = request.messages.last()
            .ok_or_else(|| anyhow!("No messages in request"))?;

        let model = self.router.route(&last_message.content);

        // Get keep_alive from cache strategy
        let keep_alive = {
            let cache = self.cache_manager.lock().await;
            cache.strategy.keep_alive_string()
        };

        // Build Ollama API request
        let ollama_request = serde_json::json!({
            "model": model,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role.as_str(),
                    "content": m.content,
                })
            }).collect::<Vec<_>>(),
            "keep_alive": keep_alive,
            "stream": request.stream,
        });

        tracing::info!(
            "Sending request to Ollama: model={}, keep_alive={}, messages={}",
            model,
            keep_alive,
            request.messages.len()
        );

        // Send to Ollama
        let response = self.http_client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&ollama_request)
            .send()
            .await
            .context("Failed to send request to Ollama")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Ollama API error ({}): {}", status, error_text);
        }

        // Parse response
        let ollama_response: serde_json::Value = response.json().await?;

        let content = ollama_response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("Invalid response format"))?
            .to_string();

        let usage = TokenUsage {
            prompt_tokens: ollama_response["usage"]["prompt_tokens"]
                .as_u64().unwrap_or(0) as usize,
            completion_tokens: ollama_response["usage"]["completion_tokens"]
                .as_u64().unwrap_or(0) as usize,
            total_tokens: ollama_response["usage"]["total_tokens"]
                .as_u64().unwrap_or(0) as usize,
        };

        // Mark model as used
        {
            let mut cache = self.cache_manager.lock().await;
            cache.mark_used(model);
        }

        Ok(ChatResponse {
            content,
            model: model.to_string(),
            usage,
        })
    }
}
```

## Phase 2: Session Management

### Components to Build

```
src/core/
├── session.rs          # Session manager with context composition
├── context.rs          # Context builder (messages + files + semantic search)
└── state.rs            # Global state
```

### Session Structure

```rust
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub channel: String,
    pub messages: Vec<StoredMessage>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct StoredMessage {
    pub role: Role,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub model_used: Option<String>,
    pub tokens: Option<usize>,
}

impl Session {
    pub async fn process_message(
        &mut self,
        content: String,
        llm_client: &OllamaClient,
        context_builder: &ContextBuilder,
    ) -> Result<String> {
        // Add user message
        self.messages.push(StoredMessage {
            role: Role::User,
            content: content.clone(),
            timestamp: Utc::now(),
            model_used: None,
            tokens: None,
        });

        // Build context (recent messages + workspace files + semantic search)
        let context = context_builder.build_context(self).await?;

        // Send to LLM
        let request = ChatRequest {
            messages: context,
            stream: false,
            max_tokens: None,
        };

        let response = llm_client.chat(request).await?;

        // Store response
        self.messages.push(StoredMessage {
            role: Role::Assistant,
            content: response.content.clone(),
            timestamp: Utc::now(),
            model_used: Some(response.model),
            tokens: Some(response.usage.completion_tokens),
        });

        self.updated_at = Utc::now();

        Ok(response.content)
    }
}
```

## Testing Plan

### 1. Unit Tests

```bash
cargo test --lib llm
```

Test coverage:
- Cache strategy keep_alive calculation
- Model routing pattern matching
- LRU eviction logic

### 2. Integration Tests

```bash
cargo test --test llm_integration
```

Test scenarios:
- Connect to local Ollama instance
- Send chat request, verify response
- Test model swapping (primary → code → primary)
- Verify context preservation across swaps

### 3. Manual Testing

```bash
# Start Ollama VM
ssh ollama@192.168.15.14

# Pull models
ollama pull qwen2.5:32b
ollama pull deepseek-coder-v2:16b
ollama pull qwen2.5:7b

# Run RustyClaw gateway
cargo run -- serve

# Test via Telegram or API
curl -X POST http://localhost:18789/api/chat \
  -d '{"message": "Hello"}' \
  -H "Content-Type: application/json"
```

## Configuration

Update `config/default.yaml`:

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

  routing:
    default: "primary"
    rules:
      - pattern: "(code|function|implement|debug|class|def |fn )"
        model: "deepseek-coder-v2:16b"
      - pattern: "^.{0,100}$"
        model: "qwen2.5:7b"
```

## Success Criteria

- ✅ Can connect to Ollama VM at 192.168.15.14:11434
- ✅ Can send chat requests and receive responses
- ✅ Model routing works (code tasks → deepseek-coder)
- ✅ Hot-swapping is transparent (context preserved)
- ✅ Cache strategy affects keep_alive parameter
- ✅ All tests pass

## Next Steps After This Phase

1. Add streaming support for real-time responses
2. Implement context composition (workspace files, semantic search)
3. Add session persistence to SQLite
4. Connect to Telegram channel adapter
