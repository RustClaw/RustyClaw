use super::{Message, Session, Storage};
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::{sqlite::SqlitePool, Row};
use std::path::Path;

#[derive(Clone)]
pub struct SqliteStorage {
    pool: SqlitePool,
}

impl SqliteStorage {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let database_url = format!("sqlite://{}", path.display());
        let pool = SqlitePool::connect(&database_url)
            .await
            .context("Failed to connect to SQLite database")?;

        // Run migrations
        sqlx::migrate!("./migrations/sqlite")
            .run(&pool)
            .await
            .context("Failed to run database migrations")?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    async fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let row = sqlx::query(
            "SELECT id, user_id, channel, scope, created_at, updated_at FROM sessions WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Session {
            id: r.get("id"),
            user_id: r.get("user_id"),
            channel: r.get("channel"),
            scope: r.get("scope"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    async fn create_session(&self, session: Session) -> Result<()> {
        sqlx::query(
            "INSERT INTO sessions (id, user_id, channel, scope, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&session.id)
        .bind(&session.user_id)
        .bind(&session.channel)
        .bind(&session.scope)
        .bind(session.created_at)
        .bind(session.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update_session(&self, session: Session) -> Result<()> {
        sqlx::query("UPDATE sessions SET updated_at = ? WHERE id = ?")
            .bind(session.updated_at)
            .bind(&session.id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn find_session(
        &self,
        user_id: &str,
        channel: &str,
        scope: &str,
    ) -> Result<Option<Session>> {
        let row = sqlx::query(
            "SELECT id, user_id, channel, scope, created_at, updated_at FROM sessions
             WHERE user_id = ? AND channel = ? AND scope = ?
             ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(user_id)
        .bind(channel)
        .bind(scope)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Session {
            id: r.get("id"),
            user_id: r.get("user_id"),
            channel: r.get("channel"),
            scope: r.get("scope"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    async fn get_messages(&self, session_id: &str, limit: Option<usize>) -> Result<Vec<Message>> {
        let limit_val = limit.unwrap_or(100);

        let rows = sqlx::query(
            "SELECT id, session_id, role, content, created_at FROM messages
             WHERE session_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(session_id)
        .bind(limit_val as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut messages: Vec<Message> = rows
            .into_iter()
            .map(|r| Message {
                id: r.get("id"),
                session_id: r.get("session_id"),
                role: r.get("role"),
                content: r.get("content"),
                created_at: r.get("created_at"),
            })
            .collect();

        messages.reverse(); // Return in chronological order
        Ok(messages)
    }

    async fn add_message(&self, message: Message) -> Result<()> {
        sqlx::query(
            "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&message.id)
        .bind(&message.session_id)
        .bind(&message.role)
        .bind(&message.content)
        .bind(message.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete_session_messages(&self, session_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM messages WHERE session_id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
