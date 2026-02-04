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
    users: Arc<Mutex<Vec<rustyclaw::storage::User>>>,
    identities: Arc<Mutex<Vec<rustyclaw::storage::Identity>>>,
    pending_links: Arc<Mutex<Vec<(String, String, String, chrono::DateTime<chrono::Utc>)>>>,
}

impl InMemoryStorage {
    fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(Vec::new())),
            messages: Arc::new(Mutex::new(Vec::new())),
            users: Arc::new(Mutex::new(Vec::new())),
            identities: Arc::new(Mutex::new(Vec::new())),
            pending_links: Arc::new(Mutex::new(Vec::new())),
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

    // Identity implementation
    async fn get_user(&self, id: &str) -> Result<Option<rustyclaw::storage::User>> {
        let users = self.users.lock().unwrap();
        Ok(users.iter().find(|u| u.id == id).cloned())
    }

    async fn get_user_by_username(&self, username: &str) -> Result<Option<rustyclaw::storage::User>> {
        let users = self.users.lock().unwrap();
        Ok(users.iter().find(|u| u.username == username).cloned())
    }

    async fn create_user(&self, user: rustyclaw::storage::User) -> Result<()> {
        let mut users = self.users.lock().unwrap();
        users.push(user);
        Ok(())
    }

    async fn user_count(&self) -> Result<usize> {
        let users = self.users.lock().unwrap();
        Ok(users.len())
    }

    async fn get_identity(
        &self,
        provider: &str,
        provider_id: &str,
    ) -> Result<Option<rustyclaw::storage::Identity>> {
        let identities = self.identities.lock().unwrap();
        Ok(identities
            .iter()
            .find(|i| i.provider == provider && i.provider_id == provider_id)
            .cloned())
    }

    async fn create_identity(&self, identity: rustyclaw::storage::Identity) -> Result<()> {
        let mut identities = self.identities.lock().unwrap();
        identities.push(identity);
        Ok(())
    }

    async fn list_identities(&self, user_id: &str) -> Result<Vec<rustyclaw::storage::Identity>> {
        let identities = self.identities.lock().unwrap();
        Ok(identities
            .iter()
            .filter(|i| i.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn create_pending_link(&self, code: &str, user_id: &str, provider: &str) -> Result<()> {
        let mut pending = self.pending_links.lock().unwrap();
        let expires_at = chrono::Utc::now() + chrono::Duration::minutes(10);
        pending.push((
            code.to_string(),
            user_id.to_string(),
            provider.to_string(),
            expires_at,
        ));
        Ok(())
    }

    async fn get_pending_link(&self, code: &str) -> Result<Option<(String, String)>> {
        let pending = self.pending_links.lock().unwrap();
        let now = chrono::Utc::now();
        Ok(pending
            .iter()
            .find(|(c, _, _, e)| c == code && *e > now)
            .map(|(_, u, p, _)| (u.clone(), p.clone())))
    }

    async fn delete_pending_link(&self, code: &str) -> Result<()> {
        let mut pending = self.pending_links.lock().unwrap();
        pending.retain(|(c, _, _, _)| c != code);
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
