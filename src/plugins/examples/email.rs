use crate::plugins::traits::{PluginApi, RustyclawPlugin, Tool, ToolResult};
use anyhow::{anyhow, Context, Result};
use lettre::transport::smtp::SmtpTransport;
use lettre::{Message, Transport};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Email provider presets for easier configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EmailProvider {
    /// Gmail SMTP settings
    Gmail,
    /// Microsoft Outlook
    Outlook,
    /// SendGrid
    SendGrid,
    /// Custom SMTP server
    Custom,
}

impl EmailProvider {
    pub fn default_server(&self) -> &str {
        match self {
            EmailProvider::Gmail => "smtp.gmail.com",
            EmailProvider::Outlook => "smtp-mail.outlook.com",
            EmailProvider::SendGrid => "smtp.sendgrid.net",
            EmailProvider::Custom => "localhost",
        }
    }

    pub fn default_port(&self) -> u16 {
        match self {
            EmailProvider::Gmail => 587,
            EmailProvider::Outlook => 587,
            EmailProvider::SendGrid => 587,
            EmailProvider::Custom => 25,
        }
    }

    pub fn default_use_tls(&self) -> bool {
        match self {
            EmailProvider::Gmail | EmailProvider::Outlook | EmailProvider::SendGrid => true,
            EmailProvider::Custom => false,
        }
    }
}

/// Email plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    /// Email provider (Gmail, Outlook, SendGrid, Custom)
    #[serde(default = "default_provider")]
    pub provider: EmailProvider,

    /// SMTP server address
    pub smtp_server: Option<String>,

    /// SMTP port
    pub smtp_port: Option<u16>,

    /// SMTP username/email
    pub smtp_username: String,

    /// SMTP password or API key
    pub smtp_password: String,

    /// Default sender email
    pub sender_email: String,

    /// Default sender name
    #[serde(default = "default_sender_name")]
    pub sender_name: String,

    /// Enable TLS
    #[serde(default = "default_use_tls")]
    pub use_tls: bool,

    /// Connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Retry attempts on failure
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: u32,

    /// Enable rich logging
    #[serde(default = "default_logging")]
    pub logging_enabled: bool,
}

fn default_provider() -> EmailProvider {
    EmailProvider::Gmail
}

fn default_sender_name() -> String {
    "RustyClaw".to_string()
}

fn default_use_tls() -> bool {
    true
}

fn default_timeout() -> u64 {
    30
}

fn default_retry_attempts() -> u32 {
    3
}

fn default_logging() -> bool {
    true
}

impl Default for EmailConfig {
    fn default() -> Self {
        let provider = EmailProvider::Gmail;
        Self {
            provider: provider.clone(),
            smtp_server: None,
            smtp_port: None,
            smtp_username: String::new(),
            smtp_password: String::new(),
            sender_email: String::new(),
            sender_name: default_sender_name(),
            use_tls: default_use_tls(),
            timeout_secs: default_timeout(),
            retry_attempts: default_retry_attempts(),
            logging_enabled: default_logging(),
        }
    }
}

impl EmailConfig {
    /// Get the SMTP server, using provider default if not specified
    fn get_server(&self) -> String {
        self.smtp_server
            .clone()
            .unwrap_or_else(|| self.provider.default_server().to_string())
    }

    /// Get the SMTP port, using provider default if not specified
    fn get_port(&self) -> u16 {
        self.smtp_port
            .unwrap_or_else(|| self.provider.default_port())
    }
}

/// Parameters for sending email
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendEmailParams {
    /// Recipient email address
    pub to: String,

    /// Email subject
    pub subject: String,

    /// Email body (plain text or HTML)
    pub body: String,

    /// Whether body is HTML
    #[serde(default)]
    pub is_html: bool,

    /// Optional CC recipients (comma-separated or array)
    #[serde(default)]
    pub cc: Option<Vec<String>>,

    /// Optional BCC recipients (comma-separated or array)
    #[serde(default)]
    pub bcc: Option<Vec<String>>,

    /// Optional reply-to email
    #[serde(default)]
    pub reply_to: Option<String>,

    /// Optional priority (high, normal, low)
    #[serde(default)]
    pub priority: Option<String>,
}

impl SendEmailParams {
    /// Validate email parameters
    fn validate(&self) -> Result<()> {
        // Validate recipient
        if !self.to.contains('@') {
            return Err(anyhow!("Invalid recipient email: {}", self.to));
        }

        if self.subject.is_empty() {
            return Err(anyhow!("Email subject cannot be empty"));
        }

        if self.body.is_empty() {
            return Err(anyhow!("Email body cannot be empty"));
        }

        // Validate CC if provided
        if let Some(ref cc_list) = self.cc {
            for email in cc_list {
                if !email.contains('@') {
                    return Err(anyhow!("Invalid CC email: {}", email));
                }
            }
        }

        // Validate BCC if provided
        if let Some(ref bcc_list) = self.bcc {
            for email in bcc_list {
                if !email.contains('@') {
                    return Err(anyhow!("Invalid BCC email: {}", email));
                }
            }
        }

        Ok(())
    }
}

/// Email plugin
pub struct EmailPlugin {
    config: Option<EmailConfig>,
}

impl EmailPlugin {
    /// Create a new email plugin
    pub fn new() -> Self {
        Self { config: None }
    }

    /// Create with configuration
    pub fn with_config(config: EmailConfig) -> Self {
        Self {
            config: Some(config),
        }
    }

    /// Send an email with retry logic
    async fn send_email(&self, params: SendEmailParams) -> Result<ToolResult> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| anyhow!("Email plugin not configured"))?;

        // Validate parameters
        params.validate()?;

        if config.logging_enabled {
            debug!("Attempting to send email to: {}", params.to);
        }

        // Retry logic with exponential backoff
        let mut last_error = None;
        for attempt in 0..config.retry_attempts {
            match self.try_send_email(config, &params).await {
                Ok(result) => {
                    if config.logging_enabled {
                        info!(
                            "Email successfully sent to: {} (attempt {})",
                            params.to,
                            attempt + 1
                        );
                    }
                    return Ok(result);
                }
                Err(e) => {
                    last_error = Some(e);
                    if attempt < config.retry_attempts - 1 {
                        let backoff_ms = (2_u64.pow(attempt) * 100).min(5000);
                        warn!(
                            "Email send failed (attempt {}), retrying in {}ms: {}",
                            attempt + 1,
                            backoff_ms,
                            last_error.as_ref().unwrap()
                        );
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("Failed to send email after retries")))
    }

    /// Try to send email once
    async fn try_send_email(
        &self,
        config: &EmailConfig,
        params: &SendEmailParams,
    ) -> Result<ToolResult> {
        // Build email message
        let mut email_builder = Message::builder()
            .from(
                format!("{} <{}>", config.sender_name, config.sender_email)
                    .parse()
                    .context("Invalid sender email format")?,
            )
            .to(params
                .to
                .parse()
                .context("Invalid recipient email format")?);

        // Add CC if provided
        if let Some(cc_list) = &params.cc {
            for cc_addr in cc_list {
                email_builder = email_builder.cc(cc_addr
                    .parse()
                    .context(format!("Invalid CC email: {}", cc_addr))?);
            }
        }

        // Add BCC if provided
        if let Some(bcc_list) = &params.bcc {
            for bcc_addr in bcc_list {
                email_builder = email_builder.bcc(
                    bcc_addr
                        .parse()
                        .context(format!("Invalid BCC email: {}", bcc_addr))?,
                );
            }
        }

        // Add reply-to if provided
        if let Some(reply_to) = &params.reply_to {
            email_builder =
                email_builder.reply_to(reply_to.parse().context("Invalid reply-to email format")?);
        }

        // Build the message
        let email = if params.is_html {
            email_builder
                .subject(&params.subject)
                .multipart(
                    lettre::message::MultiPart::alternative()
                        .singlepart(lettre::message::SinglePart::html(params.body.clone())),
                )
                .context("Failed to build HTML email")?
        } else {
            email_builder
                .subject(&params.subject)
                .singlepart(lettre::message::SinglePart::plain(params.body.clone()))
                .context("Failed to build plain text email")?
        };

        // Create SMTP transport
        let transport = self.create_smtp_transport(config)?;

        // Send email
        transport
            .send(&email)
            .context("Failed to send email via SMTP")?;

        Ok(ToolResult {
            content: format!(
                "Email successfully sent to {} with subject '{}'",
                params.to, params.subject
            ),
            details: Some(
                json!({
                    "recipient": params.to,
                    "subject": params.subject,
                    "html": params.is_html,
                    "cc_count": params.cc.as_ref().map(|c| c.len()).unwrap_or(0),
                    "bcc_count": params.bcc.as_ref().map(|b| b.len()).unwrap_or(0),
                    "provider": format!("{:?}", config.provider),
                })
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            ),
            success: true,
        })
    }

    /// Create SMTP transport with error handling
    fn create_smtp_transport(&self, config: &EmailConfig) -> Result<SmtpTransport> {
        let server = config.get_server();
        let port = config.get_port();
        let credentials = lettre::transport::smtp::authentication::Credentials::new(
            config.smtp_username.clone(),
            config.smtp_password.clone(),
        );

        let transport = if config.use_tls {
            SmtpTransport::starttls_relay(&server)
                .map_err(|e| anyhow!("Failed to create SMTP TLS connection: {}", e))?
                .port(port)
                .credentials(credentials)
                .timeout(Some(Duration::from_secs(config.timeout_secs)))
                .build()
        } else {
            SmtpTransport::builder_dangerous(&server)
                .port(port)
                .credentials(credentials)
                .timeout(Some(Duration::from_secs(config.timeout_secs)))
                .build()
        };

        Ok(transport)
    }
}

impl Default for EmailPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl RustyclawPlugin for EmailPlugin {
    fn id(&self) -> &str {
        "email"
    }

    fn name(&self) -> &str {
        "Email Plugin"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn description(&self) -> &str {
        "Professional email plugin with SMTP support, retries, validation, and comprehensive error handling. Supports Gmail, Outlook, SendGrid, and custom SMTP servers."
    }

    fn register(
        &self,
        api: &dyn PluginApi,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let plugin_self = self.clone();
        let plugin_for_tool = plugin_self.clone();

        // Create the tool before the async block to avoid lifetime issues
        let send_email_tool = Tool {
            name: "send_email".to_string(),
            description: "Send a professional email with validation, retries, and comprehensive error handling. Supports HTML/plain text, CC, BCC, and reply-to.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "string",
                        "description": "Recipient email address"
                    },
                    "subject": {
                        "type": "string",
                        "description": "Email subject line (supports markdown-like formatting)"
                    },
                    "body": {
                        "type": "string",
                        "description": "Email body content (plain text or HTML)"
                    },
                    "is_html": {
                        "type": "boolean",
                        "description": "Whether the body is HTML format",
                        "default": false
                    },
                    "cc": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional CC recipients (array of email addresses)"
                    },
                    "bcc": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional BCC recipients (array of email addresses)"
                    },
                    "reply_to": {
                        "type": "string",
                        "description": "Optional reply-to email address"
                    },
                    "priority": {
                        "type": "string",
                        "enum": ["high", "normal", "low"],
                        "description": "Optional email priority"
                    }
                },
                "required": ["to", "subject", "body"]
            }),
            execute: {
                let plugin = plugin_for_tool.clone();
                Arc::new(move |args| {
                    let plugin = plugin.clone();
                    Box::pin(async move {
                        let params: SendEmailParams = serde_json::from_str(&args)
                            .context("Failed to parse email parameters")?;
                        plugin.send_email(params).await
                    })
                })
            },
        };

        // Register tool synchronously before creating the future
        if let Err(e) = api.register_tool(send_email_tool) {
            return Box::pin(async move { Err(e) });
        }

        Box::pin(async move {
            info!("✅ Email plugin v1.0.0 loaded with send_email tool");
            info!(
                "   Provider: {:?}",
                plugin_self
                    .config
                    .as_ref()
                    .map(|c| &c.provider)
                    .unwrap_or(&EmailProvider::Gmail)
            );
            info!(
                "   Retry attempts: {}",
                plugin_self
                    .config
                    .as_ref()
                    .map(|c| c.retry_attempts)
                    .unwrap_or(default_retry_attempts())
            );

            Ok(())
        })
    }

    fn on_load(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async {
            if let Some(config) = &self.config {
                if config.logging_enabled {
                    debug!(
                        "Email plugin configured for {} account",
                        config.get_server()
                    );
                }
                info!("✅ Email plugin ready for use");
                Ok(())
            } else {
                warn!("Email plugin loaded but not configured - configure via config file");
                Ok(())
            }
        })
    }

    fn config_schema(&self) -> Option<serde_json::Value> {
        Some(json!({
            "type": "object",
            "description": "Email plugin configuration for sending emails via SMTP",
            "properties": {
                "provider": {
                    "type": "string",
                    "enum": ["gmail", "outlook", "sendgrid", "custom"],
                    "description": "Email provider (auto-configures server/port)",
                    "default": "gmail"
                },
                "smtp_server": {
                    "type": "string",
                    "description": "Custom SMTP server (overrides provider default)",
                    "examples": ["smtp.gmail.com", "smtp-mail.outlook.com"]
                },
                "smtp_port": {
                    "type": "number",
                    "description": "Custom SMTP port (overrides provider default)",
                    "examples": [587, 465, 25]
                },
                "smtp_username": {
                    "type": "string",
                    "description": "SMTP username or email address"
                },
                "smtp_password": {
                    "type": "string",
                    "description": "SMTP password or API key (use environment variable for security)"
                },
                "sender_email": {
                    "type": "string",
                    "description": "Default sender email address",
                    "examples": ["noreply@example.com"]
                },
                "sender_name": {
                    "type": "string",
                    "description": "Default sender display name",
                    "default": "RustyClaw"
                },
                "use_tls": {
                    "type": "boolean",
                    "description": "Use TLS for SMTP connection",
                    "default": true
                },
                "timeout_secs": {
                    "type": "number",
                    "description": "SMTP connection timeout in seconds",
                    "default": 30
                },
                "retry_attempts": {
                    "type": "number",
                    "description": "Number of retry attempts on failure",
                    "default": 3
                },
                "logging_enabled": {
                    "type": "boolean",
                    "description": "Enable detailed logging of email operations",
                    "default": true
                }
            },
            "required": ["smtp_username", "smtp_password", "sender_email"]
        }))
    }
}

impl Clone for EmailPlugin {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_metadata() {
        let plugin = EmailPlugin::new();
        assert_eq!(plugin.id(), "email");
        assert_eq!(plugin.name(), "Email Plugin");
        assert_eq!(plugin.version(), "1.0.0");
    }

    #[test]
    fn test_provider_defaults() {
        assert_eq!(EmailProvider::Gmail.default_server(), "smtp.gmail.com");
        assert_eq!(
            EmailProvider::Outlook.default_server(),
            "smtp-mail.outlook.com"
        );
        assert_eq!(EmailProvider::Gmail.default_port(), 587);
    }

    #[test]
    fn test_email_validation() {
        let valid_params = SendEmailParams {
            to: "user@example.com".to_string(),
            subject: "Test".to_string(),
            body: "Test body".to_string(),
            is_html: false,
            cc: None,
            bcc: None,
            reply_to: None,
            priority: None,
        };
        assert!(valid_params.validate().is_ok());

        let invalid_to = SendEmailParams {
            to: "invalid-email".to_string(),
            subject: "Test".to_string(),
            body: "Test body".to_string(),
            is_html: false,
            cc: None,
            bcc: None,
            reply_to: None,
            priority: None,
        };
        assert!(invalid_to.validate().is_err());

        let empty_subject = SendEmailParams {
            to: "user@example.com".to_string(),
            subject: String::new(),
            body: "Test body".to_string(),
            is_html: false,
            cc: None,
            bcc: None,
            reply_to: None,
            priority: None,
        };
        assert!(empty_subject.validate().is_err());
    }

    #[test]
    fn test_config_defaults() {
        let config = EmailConfig::default();
        assert_eq!(config.provider, EmailProvider::Gmail);
        assert_eq!(config.sender_name, "RustyClaw");
        assert_eq!(config.retry_attempts, 3);
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_config_schema() {
        let plugin = EmailPlugin::new();
        let schema = plugin.config_schema();
        assert!(schema.is_some());

        let schema = schema.unwrap();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["smtp_username"].is_object());
        assert!(schema["properties"]["retry_attempts"].is_object());
    }

    #[test]
    fn test_email_params_serialization() {
        let params = SendEmailParams {
            to: "user@example.com".to_string(),
            subject: "Test Subject".to_string(),
            body: "Test body content".to_string(),
            is_html: true,
            cc: Some(vec!["cc@example.com".to_string()]),
            bcc: None,
            reply_to: Some("reply@example.com".to_string()),
            priority: Some("high".to_string()),
        };

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: SendEmailParams = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.to, "user@example.com");
        assert_eq!(deserialized.subject, "Test Subject");
        assert!(deserialized.is_html);
        assert_eq!(deserialized.cc.unwrap()[0], "cc@example.com");
    }
}
