use crate::config::LlmConfig;
use anyhow::Result;
use regex::Regex;

/// Model router that selects the appropriate model based on request content
pub struct ModelRouter {
    default_model: String,
    code_model: Option<String>,
    fast_model: Option<String>,
    rules: Vec<CompiledRoutingRule>,
}

struct CompiledRoutingRule {
    pattern: Regex,
    model: String,
}

impl ModelRouter {
    pub fn new(config: &LlmConfig) -> Result<Self> {
        let mut rules = Vec::new();

        // Compile custom routing rules from config
        if let Some(routing) = &config.routing {
            for rule in &routing.rules {
                rules.push(CompiledRoutingRule {
                    pattern: Regex::new(&rule.pattern)?,
                    model: rule.model.clone(),
                });
            }
        }

        Ok(Self {
            default_model: config.models.primary.clone(),
            code_model: config.models.code.clone(),
            fast_model: config.models.fast.clone(),
            rules,
        })
    }

    /// Route a message to the appropriate model
    pub fn route(&self, message: &str) -> &str {
        // Check custom rules first (in order)
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

        // Built-in heuristics for code model
        if let Some(code_model) = &self.code_model {
            if self.is_code_related(message) {
                tracing::debug!("Routing to code model '{}'", code_model);
                return code_model;
            }
        }

        // Built-in heuristics for fast model (short messages)
        if let Some(fast_model) = &self.fast_model {
            if message.len() < 100 {
                tracing::debug!("Routing to fast model '{}' (short message)", fast_model);
                return fast_model;
            }
        }

        // Fallback to default
        tracing::debug!("Routing to default model '{}'", self.default_model);
        &self.default_model
    }

    /// Heuristics to detect code-related messages
    fn is_code_related(&self, message: &str) -> bool {
        let code_keywords = [
            "code",
            "function",
            "implement",
            "debug",
            "class",
            "def ",
            "fn ",
            "const ",
            "let ",
            "var ",
            "import ",
            "async ",
            "await ",
            "refactor",
            "bug",
            "error",
            "syntax",
        ];

        let message_lower = message.to_lowercase();
        code_keywords
            .iter()
            .any(|keyword| message_lower.contains(keyword))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LlmConfig, LlmModels, RoutingConfig, RoutingRule};

    fn test_config() -> LlmConfig {
        LlmConfig {
            provider: "ollama".to_string(),
            base_url: "http://localhost:11434/v1".to_string(),
            models: LlmModels {
                primary: "qwen2.5:32b".to_string(),
                code: Some("deepseek-coder-v2:16b".to_string()),
                fast: Some("qwen2.5:7b".to_string()),
            },
            keep_alive: None,
            cache: Default::default(),
            routing: Some(RoutingConfig {
                default: Some("qwen2.5:32b".to_string()),
                rules: vec![RoutingRule {
                    pattern: r"translate.*to.*language".to_string(),
                    model: "qwen2.5:7b".to_string(),
                }],
            }),
        }
    }

    #[test]
    fn test_default_routing() {
        let router = ModelRouter::new(&test_config()).unwrap();
        // Long message (>100 chars) that doesn't match code patterns or custom rules
        let long_message = "Please explain to me in great detail the history and cultural significance of the Renaissance period in European history.";
        assert!(long_message.len() > 100, "Message should be > 100 chars");
        let model = router.route(long_message);
        assert_eq!(model, "qwen2.5:32b");
    }

    #[test]
    fn test_code_routing() {
        let router = ModelRouter::new(&test_config()).unwrap();
        let model = router.route("Write a function to sort an array");
        assert_eq!(model, "deepseek-coder-v2:16b");
    }

    #[test]
    fn test_fast_routing() {
        let router = ModelRouter::new(&test_config()).unwrap();
        let model = router.route("Hi");
        assert_eq!(model, "qwen2.5:7b");
    }

    #[test]
    fn test_custom_rule_routing() {
        let router = ModelRouter::new(&test_config()).unwrap();
        let model = router.route("Translate this to Spanish language");
        assert_eq!(model, "qwen2.5:7b");
    }
}
