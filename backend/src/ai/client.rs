use crate::config::{AiModels, Config};
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

#[derive(Clone)]
pub struct AnthropicClient {
    client: Client,
    api_key: String,
    models: AiModels,
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
            models: AiModels::default(),
        }
    }

    pub fn from_config(config: &Config) -> Self {
        Self {
            models: config.ai_models.clone(),
            ..Self::new(&config.anthropic_api_key)
        }
    }

    pub fn models(&self) -> &AiModels {
        &self.models
    }

    /// Fail if the API reports a configured model as unknown (404).
    /// Network errors and other statuses only warn: an API hiccup must not block startup.
    pub async fn verify_models(&self) -> anyhow::Result<()> {
        let m = &self.models;
        let mut ids = vec![&m.chat, &m.ingest, &m.plan, &m.discovery];
        ids.sort();
        ids.dedup();
        for id in ids {
            let result = self
                .client
                .get(format!("https://api.anthropic.com/v1/models/{id}"))
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await;
            match result {
                Ok(r) if r.status().is_success() => {
                    tracing::info!(model = %id, "AI model verified")
                }
                Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => {
                    anyhow::bail!("configured AI model '{id}' does not exist (check AI_MODEL_*)")
                }
                Ok(r) => {
                    tracing::warn!(model = %id, status = %r.status(), "AI model check failed")
                }
                Err(e) => tracing::warn!(model = %id, "AI model check failed: {e}"),
            }
        }
        Ok(())
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
