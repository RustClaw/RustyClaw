use rustyclaw::config::workspace::Workspace;
use rustyclaw::config::{LlmConfig, LlmModels};
use rustyclaw::core::SessionManager;
use rustyclaw::llm::Client as LlmClient;
use rustyclaw::storage::sqlite::SqliteStorage;
use std::sync::Arc;
use tokio::sync::RwLock;

fn mock_llm_config() -> LlmConfig {
    LlmConfig {
        provider: "ollama".to_string(),
        base_url: "http://localhost:11434/v1".to_string(),
        models: LlmModels {
            primary: "qwen2.5:7b".to_string(),
            code: None,
            fast: None,
        },
        keep_alive: None,
        cache: Default::default(),
        routing: None,
    }
}

#[tokio::test]
async fn test_available_tools_include_creator_and_web() {
    // Setup temporary storage
    let storage = SqliteStorage::new(":memory:").await.unwrap();
    let llm_config = mock_llm_config();
    let llm_client = LlmClient::new(&llm_config).unwrap();

    // Config struct itself doesn't have Default, but its components do
    let sessions_config = rustyclaw::config::SessionsConfig {
        compaction_enabled: false,
        ..Default::default()
    };

    let config = rustyclaw::config::Config {
        llm: llm_config,
        sessions: sessions_config,
        gateway: Default::default(),
        channels: Default::default(),
        storage: Default::default(),
        logging: Default::default(),
        sandbox: Default::default(),
        tools: Default::default(),
        api: Default::default(),
        admin: Default::default(),
        workspace: Default::default(),
        agents: Default::default(),
        config_path: None,
    };

    let shared_config = Arc::new(RwLock::new(config));
    let workspace = Workspace::new(std::env::temp_dir().join("tool_test_workspace"));

    let session_manager = SessionManager::new(storage, shared_config, llm_client, workspace);

    // Initialize the plugin registry for the test
    rustyclaw::plugins::init_plugin_registry();

    let tools = session_manager.get_available_tools().await;

    let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();

    assert!(tool_names.contains(&"create_tool".to_string()));
    assert!(tool_names.contains(&"delete_tool".to_string()));
    assert!(tool_names.contains(&"web_fetch".to_string()));
    assert!(tool_names.contains(&"web_search".to_string()));
    assert!(tool_names.contains(&"exec".to_string()));
    assert!(tool_names.contains(&"bash".to_string()));
}

#[tokio::test]
async fn test_skill_registration_in_plugin_registry() {
    // Initialize registries
    rustyclaw::plugins::init_plugin_registry();

    let temp_dir = std::env::temp_dir();
    let skill_path = temp_dir.join(format!("test_skill_{}.yaml", uuid::Uuid::new_v4()));

    let skill_content = r#"---
name: test_reg_skill
description: "Test registration"
parameters:
  type: object
  properties: {}
runtime: bash
---
echo "ok"
"#;
    std::fs::write(&skill_path, skill_content).unwrap();

    let entry = rustyclaw::tools::skills::parse_skill_file(&skill_path).unwrap();
    rustyclaw::tools::skills::load_skill(entry).await.unwrap();

    // Check if it's in the plugin registry
    let registry = rustyclaw::plugins::get_plugin_registry().unwrap();
    let tool = registry.tools.get_tool("test_reg_skill").unwrap();

    assert!(tool.is_some());
    let tool = tool.unwrap();
    assert_eq!(tool.name, "test_reg_skill");

    // Cleanup
    let _ = std::fs::remove_file(&skill_path);
    rustyclaw::tools::skills::unload_skill("test_reg_skill")
        .await
        .unwrap();

    // Should be gone from plugin registry too
    let tool_after = registry.tools.get_tool("test_reg_skill").unwrap();
    assert!(tool_after.is_none());
}
