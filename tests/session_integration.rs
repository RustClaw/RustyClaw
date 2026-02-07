use rustyclaw::config::workspace::Workspace;
use rustyclaw::config::{
    CacheConfig, LlmConfig, LlmModels, RoutingConfig, RoutingRule, SessionsConfig,
};
use rustyclaw::core::{Router, SessionManager};
use rustyclaw::llm::Client as LlmClient;
use rustyclaw::storage::sqlite::SqliteStorage;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Test full session integration with LLM
#[tokio::test]
#[ignore] // Run with: cargo test session_integration -- --ignored
async fn test_session_conversation_flow() {
    // Use a test database in temp directory
    let test_db = std::env::temp_dir().join("rustyclaw_test_session.db");

    // Remove old test database if it exists
    let _ = tokio::fs::remove_file(&test_db).await;

    println!("Using test database: {}", test_db.display());

    let storage = SqliteStorage::new(&test_db)
        .await
        .expect("Failed to create storage");

    // Configure LLM
    let llm_config = LlmConfig {
        provider: "ollama".to_string(),
        base_url: "http://192.168.15.14:11434/v1".to_string(),
        models: LlmModels {
            primary: "qwen2.5:7b".to_string(), // Use fast model for testing
            code: Some("deepseek-coder-v2:16b".to_string()),
            fast: Some("qwen2.5:7b".to_string()),
        },
        keep_alive: None,
        cache: CacheConfig {
            cache_type: "ram".to_string(),
            max_models: 3,
            eviction: "lru".to_string(),
        },
        routing: Some(RoutingConfig {
            default: Some("qwen2.5:7b".to_string()),
            rules: vec![RoutingRule {
                pattern: "(code|function|implement)".to_string(),
                model: "deepseek-coder-v2:16b".to_string(),
            }],
        }),
    };

    let llm_client = LlmClient::new(&llm_config).expect("Failed to create LLM client");

    // Create session manager
    let sessions_config = SessionsConfig {
        scope: "per-sender".to_string(),
        max_tokens: 128000,
        compaction_enabled: false,
        channel_routing: "isolated".to_string(),
    };

    let full_config = rustyclaw::config::Config {
        gateway: Default::default(),
        llm: llm_config,
        channels: Default::default(),
        sessions: sessions_config,
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

    let shared_config = Arc::new(RwLock::new(full_config));
    let workspace = Workspace::new(std::env::temp_dir().join("workspace"));

    let session_manager =
        SessionManager::new(storage.clone(), shared_config, llm_client, workspace);

    // Test 1: Create session and send first message
    let session = session_manager
        .get_or_create_session("user123", "telegram", None)
        .await
        .expect("Failed to create session");

    println!("✓ Session created: {}", session.id);

    let response1 = session_manager
        .process_message(&session.id, "Hello! What's your name?", None)
        .await
        .expect("Failed to process message 1");

    println!("Response 1:");
    println!("  Model: {}", response1.model);
    println!("  Tokens: {:?}", response1.tokens);
    println!("  Content: {}", response1.content);
    println!();

    assert!(!response1.content.is_empty());
    assert!(response1.tokens.is_some());

    // Test 2: Send follow-up message (context preserved)
    let response2 = session_manager
        .process_message(&session.id, "Can you remember what I just asked you?", None)
        .await
        .expect("Failed to process message 2");

    println!("Response 2:");
    println!("  Model: {}", response2.model);
    println!("  Tokens: {:?}", response2.tokens);
    println!("  Content: {}", response2.content);
    println!();

    assert!(!response2.content.is_empty());
    // Should reference the previous question
    assert!(
        response2.content.to_lowercase().contains("name")
            || response2.content.to_lowercase().contains("asked")
    );

    // Test 3: Test model routing with code task
    let response3 = session_manager
        .process_message(&session.id, "Write a function to add two numbers", None)
        .await
        .expect("Failed to process message 3");

    println!("Response 3 (code task):");
    println!("  Model: {}", response3.model);
    println!("  Tokens: {:?}", response3.tokens);
    println!(
        "  Content preview: {}...",
        &response3.content[..100.min(response3.content.len())]
    );
    println!();

    // Should route to code model
    assert!(response3.model.contains("deepseek"));
    assert!(
        response3.content.to_lowercase().contains("def") || response3.content.contains("function")
    );

    // Test 4: Get session statistics
    let stats = session_manager
        .get_session_stats(&session.id)
        .await
        .expect("Failed to get stats");

    println!("Session Statistics:");
    println!("  Total messages: {}", stats.total_messages);
    println!("  User messages: {}", stats.user_messages);
    println!("  Assistant messages: {}", stats.assistant_messages);
    println!("  Total tokens: {}", stats.total_tokens);
    println!("  Models used: {:?}", stats.models_used);

    assert_eq!(stats.user_messages, 3);
    assert_eq!(stats.assistant_messages, 3);
    assert!(stats.total_tokens > 0);
    assert!(!stats.models_used.is_empty()); // At least one model used

    // Test 5: Clear session
    session_manager
        .clear_session(&session.id)
        .await
        .expect("Failed to clear session");

    let messages = session_manager
        .get_messages(&session.id)
        .await
        .expect("Failed to get messages");

    println!("✓ Session cleared, messages count: {}", messages.len());
    assert_eq!(messages.len(), 0);

    println!("\n✅ All session integration tests passed!");
}

/// Test Router integration
#[tokio::test]
#[ignore]
async fn test_router_conversation() {
    let test_db = std::env::temp_dir().join("rustyclaw_test_router.db");
    let _ = tokio::fs::remove_file(&test_db).await;

    let storage = SqliteStorage::new(&test_db)
        .await
        .expect("Failed to create storage");

    let llm_config = LlmConfig {
        provider: "ollama".to_string(),
        base_url: "http://192.168.15.14:11434/v1".to_string(),
        models: LlmModels {
            primary: "qwen2.5:7b".to_string(),
            code: Some("deepseek-coder-v2:16b".to_string()),
            fast: Some("qwen2.5:7b".to_string()),
        },
        keep_alive: None,
        cache: CacheConfig {
            cache_type: "ram".to_string(),
            max_models: 3,
            eviction: "lru".to_string(),
        },
        routing: None,
    };

    let llm_client = LlmClient::new(&llm_config).expect("Failed to create LLM client");

    let config = rustyclaw::config::Config {
        gateway: Default::default(),
        llm: llm_config,
        channels: Default::default(),
        sessions: SessionsConfig {
            scope: "per-sender".to_string(),
            max_tokens: 128000,
            compaction_enabled: false,
            channel_routing: "isolated".to_string(),
        },
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
    let router = Router::new(shared_config, storage, llm_client).await;

    // Test conversation through router
    let response1 = router
        .handle_message("user456", "telegram", "Hello!")
        .await
        .expect("Failed to handle message");

    println!("Router Response 1:");
    println!("  Model: {}", response1.model);
    println!("  Content: {}", response1.content);

    assert!(!response1.content.is_empty());

    // Get stats
    let stats = router
        .get_session_stats("user456", "telegram")
        .await
        .expect("Failed to get stats");

    println!("Stats: {:?}", stats);
    assert_eq!(stats.user_messages, 1);
    assert_eq!(stats.assistant_messages, 1);

    println!("\n✅ Router integration test passed!");
}
