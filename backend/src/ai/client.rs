use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

#[derive(Clone)]
pub struct AnthropicClient {
    client: Client,
    api_key: String,
}

#[derive(Debug, Serialize)]
pub struct Message {
    pub role: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct StreamEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub index: Option<u32>,
    #[serde(default)]
    pub delta: Option<serde_json::Value>,
    #[serde(default)]
    pub content_block: Option<serde_json::Value>,
}

impl AnthropicClient {
    pub fn new(api_key: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
        }
    }

    pub async fn complete(
        &self,
        model: &str,
        system: &str,
        messages: Vec<Message>,
        max_tokens: u32,
    ) -> anyhow::Result<String> {
        let body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": messages,
        });

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let json: serde_json::Value = resp.json().await?;

        // Extract text from content blocks
        let text = json["content"]
            .as_array()
            .and_then(|blocks| {
                blocks
                    .iter()
                    .find(|b| b["type"] == "text")
                    .and_then(|b| b["text"].as_str())
            })
            .unwrap_or("")
            .to_string();

        Ok(text)
    }

    pub async fn stream_raw(
        &self,
        model: &str,
        system: &str,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
        max_tokens: u32,
    ) -> anyhow::Result<std::pin::Pin<Box<dyn Stream<Item = anyhow::Result<bytes::Bytes>> + Send>>>
    {
        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": messages,
            "stream": true,
        });

        if let Some(tools) = tools {
            body["tools"] = serde_json::to_value(tools)?;
        }

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let stream = resp
            .bytes_stream()
            .map(|result| result.map_err(|e| anyhow::anyhow!(e)));

        Ok(Box::pin(stream))
    }
}
