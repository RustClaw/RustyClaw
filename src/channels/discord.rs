use crate::config::DiscordConfig;
use crate::core::Router;
use crate::storage::Storage;
use anyhow::Result;
use async_trait::async_trait;
use serenity::{model::prelude::*, prelude::*, Client};
use std::sync::Arc;

/// Discord event handler
struct DiscordHandler<S: Storage> {
    router: Arc<Router<S>>,
    config: DiscordConfig,
}

#[async_trait]
impl<S: Storage + 'static> serenity::client::EventHandler for DiscordHandler<S> {
    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore bots
        if msg.author.bot {
            return;
        }

        // Ignore webhook messages
        if msg.webhook_id.is_some() {
            return;
        }

        // Authorization check
        if !is_authorized(&msg, &self.config) {
            tracing::warn!("Unauthorized Discord user: {}", msg.author.id);
            return;
        }

        // Handle commands
        if msg.content.starts_with('/') {
            handle_command(&ctx, &msg, &self.router, &self.config).await;
            return;
        }

        // Ignore empty messages
        if msg.content.trim().is_empty() {
            return;
        }

        let user_id = msg.author.id.to_string();
        let channel = "discord";

        // Send typing indicator
        let _ = msg.channel_id.start_typing(&ctx.http);

        // Process with router
        match self
            .router
            .handle_message(&user_id, channel, &msg.content)
            .await
        {
            Ok(response) => {
                if let Err(e) = msg.channel_id.say(&ctx.http, response.content).await {
                    tracing::error!("Failed to send Discord message: {}", e);
                }
            }
            Err(e) => {
                tracing::error!("Error processing Discord message: {}", e);
                let _ = msg
                    .channel_id
                    .say(
                        &ctx.http,
                        "Sorry, I encountered an error processing your message.",
                    )
                    .await;
            }
        }
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        tracing::info!("Discord bot ready: {}", ready.user.name);
    }
}

/// Entry point
pub async fn run<S: Storage + 'static>(config: DiscordConfig, router: Router<S>) -> Result<()> {
    let token = config
        .token
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Discord token not configured"))?;

    tracing::info!("Starting Discord bot...");

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(DiscordHandler {
            router: Arc::new(router),
            config,
        })
        .await?;

    client.start().await?;
    Ok(())
}

/// Authorization check
fn is_authorized(msg: &Message, config: &DiscordConfig) -> bool {
    // Check user authorization
    if !config.allowed_users.is_empty() && !config.allowed_users.contains(&msg.author.id.get()) {
        return false;
    }

    // Check guild authorization
    if let Some(guild_id) = msg.guild_id {
        if !config.allowed_guilds.is_empty() && !config.allowed_guilds.contains(&guild_id.get()) {
            return false;
        }
    }

    true
}

/// Command handler
async fn handle_command<S: Storage>(
    ctx: &Context,
    msg: &Message,
    router: &Arc<Router<S>>,
    _config: &DiscordConfig,
) {
    let user_id = msg.author.id.to_string();
    let channel = "discord";

    let response = match msg.content.as_str() {
        "/start" | "/help" => "Available commands:\n\
            /help - Show this message\n\
            /clear - Clear conversation history\n\
            /stats - Show session statistics"
            .to_string(),
        "/clear" => match router.clear_session(&user_id, channel).await {
            Ok(_) => "Conversation history cleared!".to_string(),
            Err(e) => {
                tracing::error!("Failed to clear session: {}", e);
                "Failed to clear conversation history.".to_string()
            }
        },
        "/stats" => match router.get_session_stats(&user_id, channel).await {
            Ok(stats) => {
                let stats_msg = format!(
                    "**Session Statistics**\n\
                    Messages: {}\n\
                    Tokens used: {}",
                    stats.total_messages, stats.total_tokens
                );
                if let Err(e) = msg.channel_id.say(&ctx.http, stats_msg).await {
                    tracing::error!("Failed to send stats message: {}", e);
                }
                return;
            }
            Err(e) => {
                tracing::error!("Failed to get stats: {}", e);
                "Failed to retrieve session statistics.".to_string()
            }
        },
        _ => "Unknown command. Use /help for available commands.".to_string(),
    };

    let _ = msg.channel_id.say(&ctx.http, response).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authorization_empty_lists() {
        let config = DiscordConfig {
            enabled: true,
            token: Some("test".to_string()),
            allowed_users: vec![],
            allowed_guilds: vec![],
        };

        let msg = Message::default();
        assert!(is_authorized(&msg, &config));
    }

    #[test]
    fn test_authorization_with_allowed_users() {
        let config = DiscordConfig {
            enabled: true,
            token: Some("test".to_string()),
            allowed_users: vec![123456789],
            allowed_guilds: vec![],
        };

        // Test with matching user ID
        let mut msg = Message::default();
        msg.author.id = UserId::new(123456789);
        assert!(is_authorized(&msg, &config));

        // Test with non-matching user ID
        msg.author.id = UserId::new(987654321);
        assert!(!is_authorized(&msg, &config));
    }
}
