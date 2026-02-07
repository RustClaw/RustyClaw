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

/// Test the complete tool creation and execution flow
/// This test simulates what the LLM does:
/// 1. Create a new tool via the create_tool executor
/// 2. Verify the tool is immediately available
/// 3. Execute the newly created tool
#[tokio::test]
async fn test_llm_tool_creation_and_execution_flow() {
    use rustyclaw::tools::creator::CreateToolRequest;
    use rustyclaw::tools::executor::execute_tool_with_context;
    use serde_json::json;

    // Initialize registries
    rustyclaw::plugins::init_plugin_registry();

    // Step 1: Create a tool via the executor (simulating LLM calling create_tool)
    let create_request = CreateToolRequest {
        name: "count_words".to_string(),
        description: "Count the number of words in the input text".to_string(),
        runtime: "bash".to_string(),
        body: r#"#!/bin/bash
# Count words in the input text
echo "$1" | awk '{print NF}'"#
            .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "The text to count words in"
                }
            },
            "required": ["text"]
        }),
        policy: "allow".to_string(),
        sandbox: false, // Disable sandbox for testing simplicity
        network: false,
        timeout_secs: 10,
    };

    let create_request_json = serde_json::to_string(&create_request).unwrap();

    // Execute create_tool through the executor
    let creation_result = execute_tool_with_context(
        "create_tool",
        &create_request_json,
        Some("test-session"),
        true,
    )
    .await;

    // Verify creation was successful
    assert!(
        creation_result.is_ok(),
        "Tool creation failed: {:?}",
        creation_result.err()
    );
    let creation_msg = creation_result.unwrap();
    assert!(
        creation_msg.contains("created"),
        "Expected success message, got: {}",
        creation_msg
    );
    println!("✓ Tool created successfully: {}", creation_msg);

    // Step 2: Verify the tool is now available in the registry
    let registry =
        rustyclaw::plugins::get_plugin_registry().expect("Plugin registry not initialized");
    let count_words_tool = registry
        .tools
        .get_tool("count_words")
        .expect("Failed to get tool from registry")
        .expect("count_words tool not found in registry");

    assert_eq!(count_words_tool.name, "count_words");
    println!("✓ Tool is available in registry");

    // Step 3: Execute the newly created tool (simulating LLM calling the new tool)
    // Note: Execution may fail on Windows if bash tooling is not available
    // The important test is that the tool is created and registered
    let tool_call_json = json!({
        "text": "hello world from rustyclaw"
    })
    .to_string();

    let _execution_result =
        execute_tool_with_context("count_words", &tool_call_json, Some("test-session"), true).await;

    // On Unix-like systems, execution should work
    #[cfg(unix)]
    {
        assert!(
            _execution_result.is_ok(),
            "Tool execution failed: {:?}",
            _execution_result.err()
        );
        let output = _execution_result.unwrap().trim().to_string();
        assert_eq!(output, "4", "Expected word count 4, got: {}", output);
        println!("✓ Tool executed successfully, output: {}", output);
    }

    // On Windows, we just verify the tool is callable (execution may fail due to bash unavailability)
    #[cfg(windows)]
    {
        println!("✓ Tool execution skipped on Windows (bash not available for testing)");
        println!("  But the key feature works: tool was created and is available in registry");
    }

    // Cleanup: delete the tool
    let delete_result = rustyclaw::tools::skills::unload_skill("count_words").await;
    assert!(delete_result.is_ok(), "Failed to cleanup tool");
    println!("✓ Tool cleanup successful");
}

/// Test tool creation with Python runtime
#[tokio::test]
async fn test_create_python_tool() {
    use rustyclaw::tools::creator::CreateToolRequest;
    use rustyclaw::tools::executor::execute_tool_with_context;
    use serde_json::json;

    rustyclaw::plugins::init_plugin_registry();

    let create_request = CreateToolRequest {
        name: "reverse_text".to_string(),
        description: "Reverse the input text".to_string(),
        runtime: "python".to_string(),
        body: r#"#!/usr/bin/env python3
import sys
import json

# Read parameters from stdin
data = json.loads(sys.stdin.read())
text = data.get('text', '')
print(text[::-1])"#
            .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "The text to reverse"
                }
            },
            "required": ["text"]
        }),
        policy: "allow".to_string(),
        sandbox: false,
        network: false,
        timeout_secs: 10,
    };

    let create_request_json = serde_json::to_string(&create_request).unwrap();

    // Create the tool
    let creation_result = execute_tool_with_context(
        "create_tool",
        &create_request_json,
        Some("test-session"),
        true,
    )
    .await;

    // Tool creation should succeed - the important test is that it's created
    assert!(
        creation_result.is_ok(),
        "Python tool creation failed: {:?}",
        creation_result.err()
    );
    println!("✓ Python tool created successfully");

    // Verify it's in the registry
    let registry = rustyclaw::plugins::get_plugin_registry().unwrap();
    let tool = registry.tools.get_tool("reverse_text").unwrap();
    assert!(tool.is_some(), "reverse_text tool not found in registry");
    println!("✓ Python tool is available in registry");

    // Cleanup
    let cleanup_result = rustyclaw::tools::skills::unload_skill("reverse_text").await;
    assert!(cleanup_result.is_ok(), "Failed to cleanup Python tool");
}

/// Test that invalid tool creation is rejected
#[tokio::test]
async fn test_create_tool_validation() {
    use rustyclaw::tools::creator::CreateToolRequest;
    use rustyclaw::tools::executor::execute_tool_with_context;
    use serde_json::json;

    rustyclaw::plugins::init_plugin_registry();

    // Invalid: empty name
    let bad_request = CreateToolRequest {
        name: "".to_string(),
        description: "Bad tool".to_string(),
        runtime: "bash".to_string(),
        body: "echo test".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {}
        }),
        policy: "allow".to_string(),
        sandbox: false,
        network: false,
        timeout_secs: 10,
    };

    let json = serde_json::to_string(&bad_request).unwrap();
    let result = execute_tool_with_context("create_tool", &json, Some("test-session"), true).await;

    assert!(
        result.is_err(),
        "Expected validation error for empty name, but got success"
    );
    println!(
        "✓ Validation correctly rejected invalid tool: {:?}",
        result.err()
    );
}
