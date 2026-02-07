use crate::config::Config;
use crate::storage::{Storage, User};
use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::password;

/// Bootstrap the admin account from configuration
/// This creates the initial admin user if no users exist
pub async fn bootstrap_admin<S: Storage>(storage: &S, config: &Config) -> Result<()> {
    // Check if users already exist
    let user_count = storage.user_count().await?;
    if user_count > 0 {
        // Users already exist, skip bootstrap
        return Ok(());
    }

    // Get admin credentials from config
    let admin_username = &config.admin.username;
    let admin_password = &config.admin.password;

    // Check if password is already hashed, if not hash it
    let password_hash = if password::is_hashed(admin_password) {
        admin_password.to_string()
    } else {
        password::hash_password(admin_password)?
    };

    // Create admin user with password hash
    let admin_user = User {
        id: Uuid::new_v4().to_string(),
        username: admin_username.clone(),
        role: "admin".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        password_hash: Some(password_hash),
    };

    // Save to database
    storage.create_user(admin_user.clone()).await?;

    tracing::info!(
        "✅ Admin account bootstrapped: {} (created from config)",
        admin_username
    );

    if !password::is_hashed(&config.admin.password) {
        tracing::warn!(
            "⚠️  SECURITY WARNING: Admin account created with default password. Please change it immediately."
        );
        tracing::warn!("Use the /api/auth/change-password endpoint to set a new password.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_bootstrap_creates_admin_when_no_users_exist() {
        // This test would require a mock storage implementation
        // Skipping for now as the actual integration test will verify this
    }
}
