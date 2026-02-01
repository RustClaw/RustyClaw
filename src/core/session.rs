use crate::config::SessionsConfig;
use crate::storage::{Message as StorageMessage, Session as StorageSession, Storage};
use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

#[derive(Clone)]
pub struct SessionManager<S: Storage> {
    storage: S,
    config: SessionsConfig,
}

#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub channel: String,
}

impl<S: Storage> SessionManager<S> {
    pub fn new(storage: S, config: SessionsConfig) -> Self {
        Self { storage, config }
    }

    pub async fn get_or_create_session(&self, user_id: &str, channel: &str) -> Result<Session> {
        let scope = &self.config.scope;

        // Try to find existing session
        if let Some(session) = self.storage.find_session(user_id, channel, scope).await? {
            // Update last accessed time
            let mut updated = session.clone();
            updated.updated_at = Utc::now();
            self.storage.update_session(updated).await?;

            return Ok(Session {
                id: session.id,
                user_id: session.user_id,
                channel: session.channel,
            });
        }

        // Create new session
        let session_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let storage_session = StorageSession {
            id: session_id.clone(),
            user_id: user_id.to_string(),
            channel: channel.to_string(),
            scope: scope.clone(),
            created_at: now,
            updated_at: now,
        };

        self.storage.create_session(storage_session).await?;

        Ok(Session {
            id: session_id,
            user_id: user_id.to_string(),
            channel: channel.to_string(),
        })
    }

    pub async fn add_message(&self, session_id: &str, role: &str, content: &str) -> Result<()> {
        let message = StorageMessage {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            created_at: Utc::now(),
        };

        self.storage.add_message(message).await
    }

    pub async fn get_messages(&self, session_id: &str) -> Result<Vec<StorageMessage>> {
        self.storage.get_messages(session_id, Some(50)).await
    }

    pub async fn clear_session(&self, session_id: &str) -> Result<()> {
        self.storage.delete_session_messages(session_id).await
    }
}
