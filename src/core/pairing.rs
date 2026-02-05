use crate::storage::{Identity, Storage, User};
use anyhow::Result;
use chrono::Utc;
use qr2term::print_qr;
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

        // Print QR code for mobile apps
        // Format: rustyclaw://setup?code=XXXX
        // Note: In real world, we need IP/Host here. For now, just the code scheme.
        // The app will need to discover the IP via mDNS or user input.
        let setup_url = format!("rustyclaw://setup?code={}", code);
        println!("\nScan this QR code with the RustyClaw App to setup:");
        print_qr(&setup_url).ok();
        println!();

        Ok(Some(code))
    }
    /// Attempt to claim admin status using the setup code
    pub async fn claim_admin(&self, code: &str, username: &str) -> Result<(User, String)> {
        // 1. Verify code
        {
            let code_guard = self.setup_code.lock().unwrap();
            let current_code = code_guard
                .as_ref()
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

        // 4. Generate API Token and Identity
        let token = format!("sk-rustyclaw-{}", Uuid::new_v4());
        let identity = Identity {
            provider: "api_token".to_string(),
            provider_id: token.clone(), // TODO: Hash this in production!
            user_id: user.id.clone(),
            label: Some("Initial Admin Token".to_string()),
            created_at: Utc::now(),
            last_used_at: None,
        };
        self.storage.create_identity(identity).await?;

        // 5. Clear code
        {
            let mut code_guard = self.setup_code.lock().unwrap();
            *code_guard = None;
        }

        tracing::info!("Admin account created: {}", username);
        Ok((user, token))
    }

    /// Create an invite code for a user to link a new device
    pub async fn create_invite(&self, user_id: &str) -> Result<String> {
        let code = generate_setup_code();
        // Store in DB
        self.storage
            .create_pending_link(&code, user_id, "device_link")
            .await?;
        Ok(code)
    }

    /// Redeem an invite code to get a new API token
    pub async fn redeem_invite(&self, code: &str, label: &str) -> Result<(User, String)> {
        // 1. Validate code
        let link_data = self
            .storage
            .get_pending_link(code)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Invalid or expired invite code"))?;

        let (user_id, provider) = link_data;
        if provider != "device_link" {
            anyhow::bail!("Invalid invite type");
        }

        // 2. Get User
        let user = self
            .storage
            .get_user(&user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        // 3. Generate Token
        let token = format!("sk-rustyclaw-{}", Uuid::new_v4());
        let identity = Identity {
            provider: "api_token".to_string(),
            provider_id: token.clone(),
            user_id: user.id.clone(),
            label: Some(label.to_string()),
            created_at: Utc::now(),
            last_used_at: None,
        };
        self.storage.create_identity(identity).await?;

        // 4. Delete used code
        self.storage.delete_pending_link(code).await?;

        Ok((user, token))
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
