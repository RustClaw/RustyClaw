use crate::config::Config;
use crate::core::password;
use crate::storage::Storage;
use anyhow::{anyhow, Result};
use std::io::Write;

/// Enum for user management subcommands
pub enum UserCmd {
    Create {
        username: String,
        password: Option<String>,
    },
    Delete {
        username: String,
        force: bool,
    },
    List,
    ResetPassword {
        username: String,
        password: Option<String>,
    },
}

pub async fn handle_user_command(cmd: UserCmd, config: Config) -> Result<()> {
    match cmd {
        UserCmd::Create { username, password } => create_user(&username, password, config).await,
        UserCmd::Delete { username, force } => delete_user(&username, force, config).await,
        UserCmd::List => list_users(config).await,
        UserCmd::ResetPassword { username, password } => {
            reset_password(&username, password, config).await
        }
    }
}

async fn create_user(username: &str, password_opt: Option<String>, config: Config) -> Result<()> {
    // Validate username
    if username.is_empty() {
        return Err(anyhow!("Username cannot be empty"));
    }

    if username.len() > 64 {
        return Err(anyhow!("Username too long (max 64 characters)"));
    }

    // Get password
    let password = if let Some(pwd) = password_opt {
        pwd
    } else {
        prompt_password(&format!("Enter password for '{}': ", username))?
    };

    // Validate password
    if password.len() < 8 {
        return Err(anyhow!("Password must be at least 8 characters"));
    }

    // Initialize storage
    let storage = crate::storage::sqlite::SqliteStorage::new(&config.storage.path).await?;

    // Check if user already exists
    if let Ok(Some(_)) = storage.get_user_by_username(username).await {
        return Err(anyhow!("User '{}' already exists", username));
    }

    // Hash password
    let password_hash = password::hash_password(&password)?;

    // Create user
    let user = crate::storage::User {
        id: uuid::Uuid::new_v4().to_string(),
        username: username.to_string(),
        role: "user".to_string(),
        password_hash: Some(password_hash),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    storage.create_user(user).await?;

    println!("✓ User '{}' created successfully", username);
    Ok(())
}

async fn delete_user(username: &str, force: bool, config: Config) -> Result<()> {
    // Initialize storage
    let storage = crate::storage::sqlite::SqliteStorage::new(&config.storage.path).await?;

    // Get user to confirm exists
    let user = storage
        .get_user_by_username(username)
        .await?
        .ok_or_else(|| anyhow!("User '{}' not found", username))?;

    // Confirm deletion unless forced
    if !force {
        print!(
            "Are you sure you want to delete user '{}'? (yes/no): ",
            username
        );
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("yes") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Delete all user's tokens first
    let identities = storage.list_identities(&user.id).await?;
    for identity in identities {
        storage
            .delete_identity(&identity.provider, &identity.provider_id)
            .await?;
    }

    // Delete user
    storage.delete_user(&user.id).await?;

    println!("✓ User '{}' deleted successfully", username);
    Ok(())
}

async fn list_users(config: Config) -> Result<()> {
    // Initialize storage
    let storage = crate::storage::sqlite::SqliteStorage::new(&config.storage.path).await?;

    // Get all users
    let users = storage.list_users().await?;

    if users.is_empty() {
        println!("No users found.");
        return Ok(());
    }

    println!("\n{:<20} {:<20} {:<30}", "Username", "Role", "Created");
    println!("{}", "-".repeat(70));

    for user in users {
        println!(
            "{:<20} {:<20} {:<30}",
            user.username,
            user.role,
            user.created_at.format("%Y-%m-%d %H:%M:%S")
        );
    }

    println!();
    Ok(())
}

async fn reset_password(
    username: &str,
    password_opt: Option<String>,
    config: Config,
) -> Result<()> {
    // Initialize storage
    let storage = crate::storage::sqlite::SqliteStorage::new(&config.storage.path).await?;

    // Get user
    let user = storage
        .get_user_by_username(username)
        .await?
        .ok_or_else(|| anyhow!("User '{}' not found", username))?;

    // Get new password
    let new_password = if let Some(pwd) = password_opt {
        pwd
    } else {
        prompt_password(&format!("Enter new password for '{}': ", username))?
    };

    // Validate password
    if new_password.len() < 8 {
        return Err(anyhow!("Password must be at least 8 characters"));
    }

    // Hash new password
    let password_hash = password::hash_password(&new_password)?;

    // Update password
    storage
        .update_user_password(&user.id, password_hash)
        .await?;

    println!("✓ Password reset for user '{}' successfully", username);
    Ok(())
}

fn prompt_password(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    std::io::stdout().flush()?;

    let password = rpassword::read_password()?;

    if password.is_empty() {
        return Err(anyhow!("Password cannot be empty"));
    }

    Ok(password)
}
