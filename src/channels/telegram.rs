use crate::config::TelegramConfig;
use crate::core::Router;
use crate::storage::Storage;
use anyhow::Result;
use teloxide::{prelude::*, utils::command::BotCommands};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Available commands:")]
enum Command {
    #[command(description = "Start the bot")]
    Start,
    #[command(description = "Display help")]
    Help,
    #[command(description = "Clear conversation history")]
    Clear,
}

pub async fn run<S: Storage + 'static>(config: TelegramConfig, router: Router<S>) -> Result<()> {
    let token = config
        .token
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Telegram token not configured"))?;

    tracing::info!("Starting Telegram bot...");

    let bot = Bot::new(token);

    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(handle_command::<S>),
        )
        .branch(Update::filter_message().endpoint(handle_message::<S>));

    let mut dispatcher = Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![router, config])
        .enable_ctrlc_handler()
        .build();

    dispatcher.dispatch().await;

    Ok(())
}

async fn handle_command<S: Storage>(
    bot: Bot,
    msg: Message,
    cmd: Command,
    router: Router<S>,
    config: TelegramConfig,
) -> ResponseResult<()> {
    // Check if user is allowed
    if !config.allowed_users.is_empty() {
        let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
        if !config.allowed_users.contains(&user_id) {
            tracing::warn!("Unauthorized user attempt: {}", user_id);
            return Ok(());
        }
    }

    match cmd {
        Command::Start => {
            bot.send_message(
                msg.chat.id,
                "Welcome to RustyClaw! Send me a message to start chatting.",
            )
            .await?;
        }
        Command::Help => {
            let help_text = Command::descriptions().to_string();
            bot.send_message(msg.chat.id, help_text).await?;
        }
        Command::Clear => {
            let user_id = msg.from().map(|u| u.id.to_string()).unwrap_or_default();
            let channel = "telegram";

            if let Err(e) = router.clear_session(&user_id, channel).await {
                tracing::error!("Failed to clear session: {}", e);
                bot.send_message(msg.chat.id, "Failed to clear conversation history")
                    .await?;
            } else {
                bot.send_message(msg.chat.id, "Conversation history cleared!")
                    .await?;
            }
        }
    }

    Ok(())
}

async fn handle_message<S: Storage>(
    bot: Bot,
    msg: Message,
    router: Router<S>,
    config: TelegramConfig,
) -> ResponseResult<()> {
    // Check if user is allowed
    if !config.allowed_users.is_empty() {
        let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
        if !config.allowed_users.contains(&user_id) {
            tracing::warn!("Unauthorized user attempt: {}", user_id);
            return Ok(());
        }
    }

    let text = msg.text().unwrap_or("");
    if text.is_empty() {
        return Ok(());
    }

    let user_id = msg.from().map(|u| u.id.to_string()).unwrap_or_default();
    let channel = "telegram";

    // Send typing indicator
    bot.send_chat_action(msg.chat.id, teloxide::types::ChatAction::Typing)
        .await?;

    match router.handle_message(&user_id, channel, text).await {
        Ok(response) => {
            bot.send_message(msg.chat.id, response).await?;
        }
        Err(e) => {
            tracing::error!("Error handling message: {}", e);
            bot.send_message(
                msg.chat.id,
                "Sorry, I encountered an error processing your message.",
            )
            .await?;
        }
    }

    Ok(())
}
