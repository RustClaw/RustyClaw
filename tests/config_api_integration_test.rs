use axum::extract::State;
use axum::Json;
use rustyclaw::api::config::patch_config;
use rustyclaw::config::{Config, SessionsConfig};
use rustyclaw::core::Router;
use rustyclaw::llm::Client as LlmClient;
use rustyclaw::storage::sqlite::SqliteStorage;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_patch_config() {
    // Setup temp config path
    let test_config_path = std::env::temp_dir().join("rustyclaw_test_config.yaml");
    let _ = tokio::fs::remove_file(&test_config_path).await;

    let storage = SqliteStorage::new(":memory:")
        .await
        .expect("Failed to create storage");

    // Create minimal valid config
    let initial_config = Config {
        gateway: Default::default(),
        llm: rustyclaw::config::LlmConfig {
            provider: "ollama".to_string(),
            base_url: "http://localhost:11434".to_string(),
            models: rustyclaw::config::LlmModels {
                primary: "test".to_string(),
                code: None,
                fast: None,
            },
            keep_alive: None,
            cache: Default::default(),
            routing: None,
        },
        channels: Default::default(),
        sessions: SessionsConfig {
            scope: "per-sender".to_string(),
            max_tokens: 1000,
            compaction_enabled: false,
            channel_routing: "isolated".to_string(),
        },
        storage: Default::default(),
        logging: Default::default(),
        sandbox: Default::default(),
        tools: Default::default(),
        api: Default::default(),
        workspace: Default::default(),
        agents: Default::default(),
        config_path: Some(test_config_path.clone()),
    };

    // Save initial config to disk so save() works
    initial_config
        .save()
        .expect("Failed to save initial config");

    let shared_config = Arc::new(RwLock::new(initial_config));

    // We need an LlmClient for Router (mock/dummy)
    let llm_client =
        LlmClient::new(&shared_config.read().await.llm).expect("Failed to create LLM client");

    let router = Router::new(shared_config.clone(), storage, llm_client).await;
    let router_arc = Arc::new(router);

    // Test 1: Patch a simple field (compaction_enabled)
    let patch = json!({
        "sessions": {
            "compaction_enabled": true
        }
    });

    let result = patch_config(State(router_arc.clone()), Json(patch)).await;
    assert!(result.is_ok());

    // Verify in memory
    {
        let cfg = shared_config.read().await;
        assert!(cfg.sessions.compaction_enabled);
    }

    // Verify persistence
    let saved_content = tokio::fs::read_to_string(&test_config_path)
        .await
        .expect("Failed to read config file");
    assert!(saved_content.contains("compaction_enabled: true"));

    // Test 2: Patch nested agents (add an agent)
    let patch_agent = json!({
        "agents": {
            "agent_007": {
                "name": "Bond",
                "channels": ["secret_channel"]
            }
        }
    });

    let result = patch_config(State(router_arc.clone()), Json(patch_agent)).await;
    assert!(result.is_ok());

    // Verify in memory
    {
        let cfg = shared_config.read().await;
        let agent = cfg.agents.get("agent_007").expect("Agent not found");
        assert_eq!(agent.name, "Bond");
    }
}
