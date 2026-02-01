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
}
