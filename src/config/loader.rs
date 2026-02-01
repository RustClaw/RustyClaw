use super::Config;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let config: Config = serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

    // Perform environment variable substitution
    let config = substitute_env_vars(config)?;

    // Validate configuration
    validate_config(&config)?;

    Ok(config)
}

fn substitute_env_vars(mut config: Config) -> Result<Config> {
    // Substitute Telegram token
    if let Some(token) = &config.channels.telegram.token {
        if token.starts_with("${") && token.ends_with("}") {
            let var_name = &token[2..token.len() - 1];
            config.channels.telegram.token = std::env::var(var_name).ok();
        }
    }

    Ok(config)
}

fn validate_config(config: &Config) -> Result<()> {
    // Validate LLM config
    if config.llm.models.primary.is_empty() {
        anyhow::bail!("LLM primary model must be specified");
    }

    // Validate Telegram config
    if config.channels.telegram.enabled && config.channels.telegram.token.is_none() {
        anyhow::bail!("Telegram is enabled but no token provided");
    }

    // Validate session scope
    let valid_scopes = ["per-sender", "main", "per-peer", "per-channel-peer"];
    if !valid_scopes.contains(&config.sessions.scope.as_str()) {
        anyhow::bail!("Invalid session scope: {}", config.sessions.scope);
    }

    Ok(())
}
