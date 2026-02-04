use super::{Identity, Message, Session, Storage, User};
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
            "SELECT id, session_id, role, content, created_at, model_used, tokens FROM messages
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
            .map(|r| {
                let tokens_i64: Option<i64> = r.get("tokens");
                Message {
                    id: r.get("id"),
                    session_id: r.get("session_id"),
                    role: r.get("role"),
                    content: r.get("content"),
                    created_at: r.get("created_at"),
                    model_used: r.get("model_used"),
                    tokens: tokens_i64.map(|t| t as usize),
                }
            })
            .collect();

        messages.reverse(); // Return in chronological order
        Ok(messages)
    }

    async fn add_message(&self, message: Message) -> Result<()> {
        sqlx::query(
            "INSERT INTO messages (id, session_id, role, content, created_at, model_used, tokens)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&message.id)
        .bind(&message.session_id)
        .bind(&message.role)
        .bind(&message.content)
        .bind(message.created_at)
        .bind(&message.model_used)
        .bind(message.tokens.map(|t| t as i64))
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

    // Identity implementation
    async fn get_user(&self, id: &str) -> Result<Option<User>> {
        let row = sqlx::query(
            "SELECT id, username, role, created_at, updated_at FROM users WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| User {
            id: r.get("id"),
            username: r.get("username"),
            role: r.get("role"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    async fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        let row = sqlx::query(
            "SELECT id, username, role, created_at, updated_at FROM users WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| User {
            id: r.get("id"),
            username: r.get("username"),
            role: r.get("role"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    async fn create_user(&self, user: User) -> Result<()> {
        sqlx::query(
            "INSERT INTO users (id, username, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(user.id)
        .bind(user.username)
        .bind(user.role)
        .bind(user.created_at)
        .bind(user.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn user_count(&self) -> Result<usize> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0 as usize)
    }

    async fn get_identity(&self, provider: &str, provider_id: &str) -> Result<Option<Identity>> {
        let row = sqlx::query(
            "SELECT provider, provider_id, user_id, label, created_at, last_used_at FROM identities WHERE provider = ? AND provider_id = ?"
        )
        .bind(provider)
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Identity {
            provider: r.get("provider"),
            provider_id: r.get("provider_id"),
            user_id: r.get("user_id"),
            label: r.get("label"),
            created_at: r.get("created_at"),
            last_used_at: r.get("last_used_at"),
        }))
    }

    async fn create_identity(&self, identity: Identity) -> Result<()> {
        sqlx::query(
            "INSERT INTO identities (provider, provider_id, user_id, label, created_at, last_used_at) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(identity.provider)
        .bind(identity.provider_id)
        .bind(identity.user_id)
        .bind(identity.label)
        .bind(identity.created_at)
        .bind(identity.last_used_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_identities(&self, user_id: &str) -> Result<Vec<Identity>> {
        let rows = sqlx::query(
            "SELECT provider, provider_id, user_id, label, created_at, last_used_at FROM identities WHERE user_id = ?"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Identity {
                provider: r.get("provider"),
                provider_id: r.get("provider_id"),
                user_id: r.get("user_id"),
                label: r.get("label"),
                created_at: r.get("created_at"),
                last_used_at: r.get("last_used_at"),
            })
            .collect())
    }

    async fn create_pending_link(&self, code: &str, user_id: &str, provider: &str) -> Result<()> {
        // Expiry in 10 minutes
        let expires_at = chrono::Utc::now() + chrono::Duration::minutes(10);
        sqlx::query(
            "INSERT INTO pending_links (code, user_id, provider, expires_at) VALUES (?, ?, ?, ?)",
        )
        .bind(code)
        .bind(user_id)
        .bind(provider)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_pending_link(&self, code: &str) -> Result<Option<(String, String)>> {
        let row = sqlx::query(
            "SELECT user_id, provider FROM pending_links WHERE code = ? AND expires_at > datetime('now')"
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| (r.get("user_id"), r.get("provider"))))
    }

    async fn delete_pending_link(&self, code: &str) -> Result<()> {
        sqlx::query("DELETE FROM pending_links WHERE code = ?")
            .bind(code)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
