pub mod sqlite;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub channel: String,
    pub scope: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_used: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub provider: String,
    pub provider_id: String,
    pub user_id: String,
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait Storage: Send + Sync + Clone {
    async fn get_session(&self, id: &str) -> Result<Option<Session>>;
    async fn create_session(&self, session: Session) -> Result<()>;
    async fn update_session(&self, session: Session) -> Result<()>;
    async fn find_session(
        &self,
        user_id: &str,
        channel: &str,
        scope: &str,
    ) -> Result<Option<Session>>;

    async fn get_messages(&self, session_id: &str, limit: Option<usize>) -> Result<Vec<Message>>;
    async fn add_message(&self, message: Message) -> Result<()>;
    async fn delete_session_messages(&self, session_id: &str) -> Result<()>;

    // User & Identity Management
    async fn get_user(&self, id: &str) -> Result<Option<User>>;
    async fn get_user_by_username(&self, username: &str) -> Result<Option<User>>;
    async fn create_user(&self, user: User) -> Result<()>;
    async fn user_count(&self) -> Result<usize>;
    async fn list_users(&self) -> Result<Vec<User>>;
    async fn delete_user(&self, user_id: &str) -> Result<()>;

    async fn get_identity(&self, provider: &str, provider_id: &str) -> Result<Option<Identity>>;
    async fn create_identity(&self, identity: Identity) -> Result<()>;
    async fn list_identities(&self, user_id: &str) -> Result<Vec<Identity>>;

    // Pending Links (OTP)
    async fn create_pending_link(&self, code: &str, user_id: &str, provider: &str) -> Result<()>;
    async fn get_pending_link(&self, code: &str) -> Result<Option<(String, String)>>; // returns (user_id, provider)
    async fn delete_pending_link(&self, code: &str) -> Result<()>;

    // Password management
    async fn update_user_password(&self, user_id: &str, password_hash: String) -> Result<()>;

    // Identity management
    async fn delete_identity(&self, provider: &str, provider_id: &str) -> Result<()>;
}
