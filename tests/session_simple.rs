use anyhow::Result;
use async_trait::async_trait;
/// Simple test to verify session integration compiles and basic logic works
/// This doesn't test the full database integration
use rustyclaw::config::{CacheConfig, LlmConfig, LlmModels, SessionsConfig};
use rustyclaw::core::SessionManager;
use rustyclaw::llm::Client as LlmClient;
use rustyclaw::storage::{Message, Session, Storage};
use std::sync::{Arc, Mutex};

/// In-memory storage for testing (no SQLite)
#[derive(Clone)]
struct InMemoryStorage {
    sessions: Arc<Mutex<Vec<Session>>>,
    messages: Arc<Mutex<Vec<Message>>>,
}

impl InMemoryStorage {
    fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(Vec::new())),
            messages: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl Storage for InMemoryStorage {
    async fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let sessions = self.sessions.lock().unwrap();
        Ok(sessions.iter().find(|s| s.id == id).cloned())
    }

    async fn create_session(&self, session: Session) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.push(session);
        Ok(())
    }

    async fn update_session(&self, session: Session) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(s) = sessions.iter_mut().find(|s| s.id == session.id) {
            *s = session;
        }
        Ok(())
    }

    async fn find_session(
        &self,
        user_id: &str,
        channel: &str,
        scope: &str,
    ) -> Result<Option<Session>> {
        let sessions = self.sessions.lock().unwrap();
        Ok(sessions
            .iter()
            .find(|s| s.user_id == user_id && s.channel == channel && s.scope == scope)
            .cloned())
    }

    async fn get_messages(&self, session_id: &str, limit: Option<usize>) -> Result<Vec<Message>> {
        let messages = self.messages.lock().unwrap();
        let mut session_messages: Vec<Message> = messages
            .iter()
            .filter(|m| m.session_id == session_id)
            .cloned()
            .collect();

        if let Some(lim) = limit {
            session_messages.truncate(lim);
        }

        Ok(session_messages)
    }

    async fn add_message(&self, message: Message) -> Result<()> {
        let mut messages = self.messages.lock().unwrap();
        messages.push(message);
        Ok(())
    }

    async fn delete_session_messages(&self, session_id: &str) -> Result<()> {
        let mut messages = self.messages.lock().unwrap();
        messages.retain(|m| m.session_id != session_id);
        Ok(())
    }
}

#[tokio::test]
#[ignore]
async fn test_session_with_memory_storage() {
    let storage = InMemoryStorage::new();

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

    let sessions_config = SessionsConfig {
        scope: "per-sender".to_string(),
        max_tokens: 128000,
        channel_routing: "isolated".to_string(),
    };

    let session_manager = SessionManager::new(storage, sessions_config, llm_client);

    println!("Testing session with in-memory storage...");

    // Create session
    let session = session_manager
        .get_or_create_session("test_user", "test_channel")
        .await
        .expect("Failed to create session");

    println!("✓ Session created: {}", session.id);

    // Process message
    let response = session_manager
        .process_message(&session.id, "Hello! Say hi in one sentence.")
        .await
        .expect("Failed to process message");

    println!("✓ Response received:");
    println!("  Model: {}", response.model);
    println!("  Tokens: {:?}", response.tokens);
    println!("  Content: {}", response.content);

    assert!(!response.content.is_empty());
    assert!(response.tokens.is_some());

    // Get stats
    let stats = session_manager
        .get_session_stats(&session.id)
        .await
        .expect("Failed to get stats");

    println!("✓ Session stats:");
    println!("  Messages: {}", stats.total_messages);
    println!("  Tokens: {}", stats.total_tokens);
    println!("  Models: {:?}", stats.models_used);

    assert_eq!(stats.user_messages, 1);
    assert_eq!(stats.assistant_messages, 1);

    println!("\n✅ In-memory session test passed!");
}
