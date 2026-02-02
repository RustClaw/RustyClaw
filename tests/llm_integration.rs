use rustyclaw::config::{CacheConfig, LlmConfig, LlmModels, RoutingConfig, RoutingRule};
use rustyclaw::llm::{ChatMessage, ChatRequest, Client};

/// Test actual connection to Ollama VM
#[tokio::test]
#[ignore] // Run with: cargo test --test llm_integration -- --ignored
async fn test_ollama_connection() {
    let config = LlmConfig {
        provider: "ollama".to_string(),
        base_url: "http://192.168.15.14:11434/v1".to_string(),
        models: LlmModels {
            primary: "qwen2.5:32b".to_string(),
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

    let client = Client::new(&config).expect("Failed to create client");

    // Test 1: Simple chat with fast model
    let request = ChatRequest {
        model: "qwen2.5:7b".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Say 'Hello from RustyClaw!' in one sentence.".to_string(),
        }],
        max_tokens: Some(50),
        temperature: None,
    };

    let response = client.chat(request).await.expect("Failed to get response");

    println!("Response from qwen2.5:7b:");
    println!("  Model: {}", response.model);
    println!("  Content: {}", response.content);
    println!("  Usage: {:?}", response.usage);

    assert!(!response.content.is_empty());
    assert!(response.content.to_lowercase().contains("hello"));
}

/// Test model routing
#[tokio::test]
#[ignore]
async fn test_model_routing() {
    let config = LlmConfig {
        provider: "ollama".to_string(),
        base_url: "http://192.168.15.14:11434/v1".to_string(),
        models: LlmModels {
            primary: "qwen2.5:32b".to_string(),
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
            default: Some("qwen2.5:32b".to_string()),
            rules: vec![RoutingRule {
                pattern: "(code|function|implement)".to_string(),
                model: "deepseek-coder-v2:16b".to_string(),
            }],
        }),
    };

    let client = Client::new(&config).expect("Failed to create client");

    // Test automatic routing to code model
    let request = ChatRequest {
        model: "".to_string(), // Empty = auto-route
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Write a function to add two numbers".to_string(),
        }],
        max_tokens: Some(100),
        temperature: None,
    };

    let response = client.chat(request).await.expect("Failed to get response");

    println!("Response (should use deepseek-coder):");
    println!("  Model: {}", response.model);
    println!("  Content: {}", response.content);

    // Should route to code model
    assert!(response.model.contains("deepseek"));
}

/// Test hot-swapping between models
#[tokio::test]
#[ignore]
async fn test_hot_swapping() {
    let config = LlmConfig {
        provider: "ollama".to_string(),
        base_url: "http://192.168.15.14:11434/v1".to_string(),
        models: LlmModels {
            primary: "qwen2.5:32b".to_string(),
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

    let client = Client::new(&config).expect("Failed to create client");

    println!("Test 1: Fast model (qwen2.5:7b)");
    let start = std::time::Instant::now();
    let request1 = ChatRequest {
        model: "qwen2.5:7b".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Say hi".to_string(),
        }],
        max_tokens: Some(20),
        temperature: None,
    };
    let response1 = client.chat(request1).await.expect("Failed to get response");
    let time1 = start.elapsed();
    println!("  Time: {:?}", time1);
    println!("  Response: {}", response1.content);

    println!("\nTest 2: Code model (deepseek-coder-v2:16b) - swap");
    let start = std::time::Instant::now();
    let request2 = ChatRequest {
        model: "deepseek-coder-v2:16b".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Say hi".to_string(),
        }],
        max_tokens: Some(20),
        temperature: None,
    };
    let response2 = client.chat(request2).await.expect("Failed to get response");
    let time2 = start.elapsed();
    println!("  Time: {:?} (should include swap time)", time2);
    println!("  Response: {}", response2.content);

    println!("\nTest 3: Back to fast model - swap again");
    let start = std::time::Instant::now();
    let request3 = ChatRequest {
        model: "qwen2.5:7b".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Say hi".to_string(),
        }],
        max_tokens: Some(20),
        temperature: None,
    };
    let response3 = client.chat(request3).await.expect("Failed to get response");
    let time3 = start.elapsed();
    println!(
        "  Time: {:?} (should be fast - likely cached in RAM)",
        time3
    );
    println!("  Response: {}", response3.content);

    // All should succeed
    assert!(!response1.content.is_empty());
    assert!(!response2.content.is_empty());
    assert!(!response3.content.is_empty());
}
