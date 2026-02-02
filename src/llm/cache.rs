use crate::config::LlmConfig;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Cache strategy determines how long models stay loaded
#[derive(Debug, Clone)]
pub enum CacheStrategy {
    /// Keep models in RAM - fast swapping (~1-2 sec)
    Ram { keep_alive: Duration },
    /// Unload to SSD quickly - slower swapping (~20-30 sec)
    Ssd { keep_alive: Duration },
    /// No caching - always reload from SSD
    None,
}

impl CacheStrategy {
    /// Create cache strategy from configuration
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

    /// Get keep_alive parameter for Ollama API
    pub fn keep_alive_string(&self) -> String {
        match self {
            CacheStrategy::Ram { keep_alive } => {
                let minutes = keep_alive.as_secs() / 60;
                format!("{}m", minutes)
            }
            CacheStrategy::Ssd { keep_alive } => {
                let minutes = keep_alive.as_secs() / 60;
                format!("{}m", minutes)
            }
            CacheStrategy::None => "0".to_string(),
        }
    }
}

/// Manages model cache tracking and LRU eviction
pub struct CacheManager {
    pub strategy: CacheStrategy,
    loaded_models: HashMap<String, Instant>,
    max_models: usize,
}

impl CacheManager {
    pub fn new(config: &LlmConfig) -> Self {
        Self {
            strategy: CacheStrategy::from_config(config),
            loaded_models: HashMap::new(),
            max_models: config.cache.max_models,
        }
    }

    /// Mark a model as used (updates LRU tracking)
    pub fn mark_used(&mut self, model: &str) {
        self.loaded_models.insert(model.to_string(), Instant::now());

        // Evict if we're over the limit
        if self.loaded_models.len() > self.max_models {
            self.evict_lru();
        }
    }

    /// Evict the least recently used model
    fn evict_lru(&mut self) {
        if let Some((lru_model, _)) = self
            .loaded_models
            .iter()
            .min_by_key(|(_, last_used)| *last_used)
        {
            let model = lru_model.clone();
            self.loaded_models.remove(&model);
            tracing::debug!("Evicted LRU model from cache tracking: {}", model);
        }
    }

    /// Get currently loaded models
    pub fn loaded_models(&self) -> Vec<String> {
        self.loaded_models.keys().cloned().collect()
    }

    /// Get keep_alive string for API requests
    pub fn keep_alive(&self) -> String {
        self.strategy.keep_alive_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CacheConfig, LlmConfig, LlmModels};

    fn test_config(cache_type: &str) -> LlmConfig {
        LlmConfig {
            provider: "ollama".to_string(),
            base_url: "http://localhost:11434/v1".to_string(),
            models: LlmModels {
                primary: "qwen2.5:32b".to_string(),
                code: Some("deepseek-coder-v2:16b".to_string()),
                fast: Some("qwen2.5:7b".to_string()),
            },
            keep_alive: None,
            cache: CacheConfig {
                cache_type: cache_type.to_string(),
                max_models: 3,
                eviction: "lru".to_string(),
            },
            routing: None,
        }
    }

    #[test]
    fn test_ram_cache_strategy() {
        let config = test_config("ram");
        let strategy = CacheStrategy::from_config(&config);
        assert_eq!(strategy.keep_alive_string(), "30m");
    }

    #[test]
    fn test_ssd_cache_strategy() {
        let config = test_config("ssd");
        let strategy = CacheStrategy::from_config(&config);
        assert_eq!(strategy.keep_alive_string(), "2m");
    }

    #[test]
    fn test_none_cache_strategy() {
        let config = test_config("none");
        let strategy = CacheStrategy::from_config(&config);
        assert_eq!(strategy.keep_alive_string(), "0");
    }

    #[test]
    fn test_lru_eviction() {
        let config = test_config("ram");
        let mut manager = CacheManager::new(&config);

        manager.mark_used("model1");
        std::thread::sleep(std::time::Duration::from_millis(10));
        manager.mark_used("model2");
        std::thread::sleep(std::time::Duration::from_millis(10));
        manager.mark_used("model3");
        std::thread::sleep(std::time::Duration::from_millis(10));

        // This should evict model1 (least recently used)
        manager.mark_used("model4");

        let loaded = manager.loaded_models();
        assert_eq!(loaded.len(), 3);
        assert!(!loaded.contains(&"model1".to_string()));
        assert!(loaded.contains(&"model2".to_string()));
        assert!(loaded.contains(&"model3".to_string()));
        assert!(loaded.contains(&"model4".to_string()));
    }
}
