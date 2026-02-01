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
    Serve,
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
