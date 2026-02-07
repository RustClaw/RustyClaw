use anyhow::Result;
use clap::{Parser, Subcommand};
use rustyclaw::Config;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "rustyclaw")]
#[command(about = "A local-first, privacy-focused AI assistant gateway", long_about = None)]
struct Cli {
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the RustyClaw gateway server
    Serve,

    /// Manage channel connections (WhatsApp, Discord, etc.)
    #[command(subcommand)]
    Channels(ChannelsCommands),

    /// Manage users and authentication
    #[command(subcommand)]
    User(UserCommands),

    /// Manage API tokens
    #[command(subcommand)]
    Token(TokenCommands),
}

#[derive(Subcommand)]
enum ChannelsCommands {
    /// Connect a channel (e.g., `rustyclaw channels connect whatsapp`)
    Connect {
        /// Channel to connect (whatsapp, discord, etc.)
        #[arg(value_name = "CHANNEL")]
        channel: String,
    },
}

#[derive(Subcommand)]
enum UserCommands {
    /// Create a new user
    Create {
        /// Username
        #[arg(long)]
        username: String,

        /// Password (if not provided, will be prompted)
        #[arg(long)]
        password: Option<String>,
    },

    /// Delete a user
    Delete {
        /// Username to delete
        #[arg(long)]
        username: String,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// List all users
    List,

    /// Reset a user's password
    #[command(name = "reset-password")]
    ResetPassword {
        /// Username whose password to reset
        #[arg(long)]
        username: String,

        /// New password (if not provided, will be prompted)
        #[arg(long)]
        password: Option<String>,
    },
}

#[derive(Subcommand)]
enum TokenCommands {
    /// List a user's API tokens
    List {
        /// Username
        #[arg(long)]
        username: String,
    },

    /// Revoke an API token
    Revoke {
        /// Token ID to revoke
        #[arg(long)]
        token_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let config_path = cli.config.unwrap_or_else(|| {
        let default_path = Config::default_path();
        if default_path.exists() {
            default_path
        } else {
            PathBuf::from("config/default.yaml")
        }
    });

    let config = if config_path.exists() {
        Config::load(&config_path)?
    } else {
        eprintln!("Config file not found: {}", config_path.display());
        eprintln!("Please create a config file or use --config to specify one.");
        eprintln!("See config/default.yaml for an example.");
        std::process::exit(1);
    };

    // Initialize logging
    init_logging(&config.logging.level, &config.logging.format)?;

    tracing::info!("RustyClaw starting...");
    tracing::info!("Config loaded from: {}", config_path.display());

    match cli.command {
        Some(Commands::Serve) | None => {
            rustyclaw::run(config).await?;
        }
        Some(Commands::Channels(ChannelsCommands::Connect { channel })) => {
            rustyclaw::channels::connect(&channel, config).await?;
        }
        Some(Commands::User(user_cmd)) => {
            let cmd = match user_cmd {
                UserCommands::Create { username, password } => {
                    rustyclaw::cli::user::UserCmd::Create { username, password }
                }
                UserCommands::Delete { username, force } => {
                    rustyclaw::cli::user::UserCmd::Delete { username, force }
                }
                UserCommands::List => rustyclaw::cli::user::UserCmd::List,
                UserCommands::ResetPassword { username, password } => {
                    rustyclaw::cli::user::UserCmd::ResetPassword { username, password }
                }
            };
            rustyclaw::cli::user::handle_user_command(cmd, config).await?;
        }
        Some(Commands::Token(token_cmd)) => {
            let cmd = match token_cmd {
                TokenCommands::List { username } => {
                    rustyclaw::cli::token::TokenCmd::List { username }
                }
                TokenCommands::Revoke { token_id } => {
                    rustyclaw::cli::token::TokenCmd::Revoke { token_id }
                }
            };
            rustyclaw::cli::token::handle_token_command(cmd, config).await?;
        }
    }

    Ok(())
}

fn init_logging(level: &str, format: &str) -> Result<()> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));

    match format {
        "json" => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().json())
                .init();
        }
        "compact" => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().compact())
                .init();
        }
        _ => {
            // Default to pretty
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().pretty())
                .init();
        }
    }

    Ok(())
}
