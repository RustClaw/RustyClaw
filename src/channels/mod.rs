use anyhow::Result;

pub mod telegram;
pub mod whatsapp;

pub use whatsapp::WhatsAppAdapter;

/// Connect to a channel (CLI command handler)
pub async fn connect(channel: &str, _config: crate::Config) -> Result<()> {
    match channel {
        "whatsapp" => {
            whatsapp::connect_whatsapp_cli().await?;
        }
        other => {
            anyhow::bail!("Unknown channel: {}. Supported: whatsapp", other);
        }
    }

    Ok(())
}
