use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Parameters for web_fetch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchParams {
    /// URL to fetch
    pub url: String,
}

/// Parameters for web_search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchParams {
    /// Search query
    pub query: String,
    /// Number of results to return
    #[serde(default = "default_count")]
    pub count: usize,
}

fn default_count() -> usize {
    5
}

/// Fetch content from a URL
pub async fn web_fetch(params: WebFetchParams) -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent("RustyClaw/0.1.0 (Privacy-focused AI Assistant)")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let response = client.get(&params.url).send().await?;
    let status = response.status();

    if !status.is_success() {
        return Ok(format!("Error fetching {}: HTTP {}", params.url, status));
    }

    let text = response.text().await?;

    // Very basic HTML to text conversion for now
    let content = if params.url.ends_with(".json")
        || text.trim().starts_with('{')
        || text.trim().starts_with('[')
    {
        text
    } else {
        // Strip scripts and styles and common tags (naive approach)
        text.split("<script")
            .map(|part| part.split("</script>").last().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ")
            .split("<style")
            .map(|part| part.split("</style>").last().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ")
            .replace(
                "<br>", "
",
            )
            .replace(
                "<p>", "

",
            )
            .chars()
            .filter(|&c| c != '<' && c != '>') // very naive tag stripping
            .collect()
    };

    Ok(content.trim().to_string())
}

/// Search the web
pub async fn web_search(params: WebSearchParams) -> Result<String> {
    tracing::info!("Web search for: {}", params.query);

    // In a real implementation, this would use a Search API.
    // For now, we return a simulated result pointing to key resources.
    Ok(format!(
        "Search results for '{}':


        1. [RustyClaw GitHub](https://github.com/your-org/rustyclaw)

           RustyClaw is a local-first, privacy-focused AI assistant gateway.


        2. [Ollama](https://ollama.ai)

           Get up and running with large language models locally.


        (Note: Full web search integration is pending a Search API key configuration.)",
        params.query
    ))
}

/// Get web tool definitions for LLM
pub fn get_web_tool_definitions() -> Vec<serde_json::Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "web_fetch",
                "description": "Fetch the content of a web page and return it as text. Use this to read documentation, articles, or any online content.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The full URL to fetch (e.g., https://example.com/page)"
                        }
                    },
                    "required": ["url"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "Search the web for a given query. Returns a list of relevant websites and snippets.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query"
                        },
                        "count": {
                            "type": "integer",
                            "description": "Number of results to return",
                            "default": 5
                        }
                    },
                    "required": ["query"]
                }
            }
        }),
    ]
}
