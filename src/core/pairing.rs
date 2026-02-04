use crate::storage::{Storage, User};
use anyhow::{Context, Result};
use chrono::Utc;
use rand::Rng;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Manages the initial setup and device pairing flows
#[derive(Clone)]
pub struct PairingManager<S: Storage> {
    storage: S,
    // In-memory store for the initial admin setup code
    // We use a mutex because this is shared state
    setup_code: Arc<Mutex<Option<String>>>,
}

impl<S: Storage + 'static> PairingManager<S> {
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            setup_code: Arc::new(Mutex::new(None)),
        }
    }

    /// Check if the system needs setup (i.e., no users exist)
    /// If so, generates and returns a setup code
    pub async fn check_and_start_setup(&self) -> Result<Option<String>> {
        let count = self.storage.user_count().await?;
        if count > 0 {
            return Ok(None);
        }

        // No users - we are in setup mode
        let mut code_guard = self.setup_code.lock().unwrap();
        
        // If we already have a code, return it
        if let Some(code) = code_guard.as_ref() {
            return Ok(Some(code.clone()));
        }

        // Generate a new 8-char alphanumeric code
        let code = generate_setup_code();
        *code_guard = Some(code.clone());

        tracing::warn!("⚠️  INITIAL SETUP REQUIRED ⚠️");
        tracing::warn!("Use this code to create the Admin account: {}", code);
        
        Ok(Some(code))
    }

    /// Attempt to claim admin status using the setup code
    pub async fn claim_admin(&self, code: &str, username: &str) -> Result<User> {
        // 1. Verify code
        {
            let code_guard = self.setup_code.lock().unwrap();
            let current_code = code_guard.as_ref()
                .ok_or_else(|| anyhow::anyhow!("Setup mode is not active"))?;
            
            if current_code != code {
                anyhow::bail!("Invalid setup code");
            }
        }

        // 2. Verify no users exist (double check race condition)
        if self.storage.user_count().await? > 0 {
            anyhow::bail!("Admin account already exists");
        }

        // 3. Create Admin User
        let user = User {
            id: Uuid::new_v4().to_string(),
            username: username.to_string(),
            role: "admin".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        self.storage.create_user(user.clone()).await?;

        // 4. Clear code
        {
            let mut code_guard = self.setup_code.lock().unwrap();
            *code_guard = None;
        }

        tracing::info!("Admin account created: {}", username);
        Ok(user)
    }
}

fn generate_setup_code() -> String {
    use rand::distributions::Alphanumeric;
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect::<String>()
        .to_uppercase()
}
