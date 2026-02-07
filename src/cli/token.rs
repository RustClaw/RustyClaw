use crate::config::Config;
use crate::storage::Storage;
use anyhow::{anyhow, Result};

/// Enum for token management subcommands
pub enum TokenCmd {
    List { username: String },
    Revoke { token_id: String },
}

pub async fn handle_token_command(cmd: TokenCmd, config: Config) -> Result<()> {
    match cmd {
        TokenCmd::List { username } => list_tokens(&username, config).await,
        TokenCmd::Revoke { token_id } => revoke_token(&token_id, config).await,
    }
}

async fn list_tokens(username: &str, config: Config) -> Result<()> {
    // Initialize storage
    let storage = crate::storage::sqlite::SqliteStorage::new(&config.storage.path).await?;

    // Get user
    let user = storage
        .get_user_by_username(username)
        .await?
        .ok_or_else(|| anyhow!("User '{}' not found", username))?;

    // Get user's identities
    let identities = storage.list_identities(&user.id).await?;

    // Filter to API tokens only
    let tokens: Vec<_> = identities
        .into_iter()
        .filter(|id| id.provider == "api_token")
        .collect();

    if tokens.is_empty() {
        println!("User '{}' has no API tokens.", username);
        return Ok(());
    }

    println!("\nTokens for user '{}':", username);
    println!("{:<40} {:<30} {:<20}", "Token ID", "Label", "Created");
    println!("{}", "-".repeat(90));

    for token in tokens {
        let label = token.label.unwrap_or_default();
        println!(
            "{:<40} {:<30} {:<20}",
            token.provider_id,
            label,
            token.created_at.format("%Y-%m-%d %H:%M:%S")
        );
    }

    println!();
    Ok(())
}

async fn revoke_token(token_id: &str, config: Config) -> Result<()> {
    // Initialize storage
    let storage = crate::storage::sqlite::SqliteStorage::new(&config.storage.path).await?;

    // Get the token identity
    let identity = storage
        .get_identity("api_token", token_id)
        .await?
        .ok_or_else(|| anyhow!("Token '{}' not found", token_id))?;

    // Delete the token
    storage
        .delete_identity(&identity.provider, &identity.provider_id)
        .await?;

    println!("✓ Token '{}' revoked successfully", token_id);
    Ok(())
}
