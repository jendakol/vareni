use crate::ai::client::{AnthropicClient, Message};
use crate::models::{IngredientInput, StepInput};
use serde::{Deserialize, Serialize};

const INGEST_MODEL: &str = "claude-haiku-4-5-20251001";
const INGEST_SYSTEM: &str = r#"You are a recipe parser. Extract the recipe from the user's input and return ONLY valid JSON.
No preamble, no markdown, no explanation. Schema:
{
  "title": string,
  "description": string | null,
  "servings": number | null,
  "prep_time_min": number | null,
  "cook_time_min": number | null,
  "tags": [string],
  "ingredients": [{ "name": string, "amount": number | null, "unit": string | null, "note": string | null }],
  "steps": [{ "step_order": number, "instruction": string }]
}

For tags: infer relevant categories from the recipe content. Examples: "quick", "vegetarian",
"vegan", "soup", "salad", "pasta", "Asian", "Czech", "dessert", "breakfast", "one-pot".
Assign 1-5 tags."#;

#[derive(Debug, Serialize, Deserialize)]
pub struct ParsedRecipe {
    pub title: String,
    pub description: Option<String>,
    pub servings: Option<i32>,
    pub prep_time_min: Option<i32>,
    pub cook_time_min: Option<i32>,
    pub tags: Vec<String>,
    pub ingredients: Vec<IngredientInput>,
    pub steps: Vec<StepInput>,
}

pub async fn parse_text(client: &AnthropicClient, text: &str) -> anyhow::Result<ParsedRecipe> {
    let messages = vec![Message {
        role: "user".into(),
        content: serde_json::json!(text),
    }];

    let response = client
        .complete(INGEST_MODEL, INGEST_SYSTEM, messages, 4096)
        .await?;
    let json_str = extract_json(&response);
    let parsed: ParsedRecipe = serde_json::from_str(json_str)?;
    Ok(parsed)
}

/// Strip markdown code fences and find the JSON object in Claude's response.
fn extract_json(response: &str) -> &str {
    let trimmed = response.trim();
    if let Some(start) = trimmed.find('{') {
        let end = trimmed.rfind('}').map(|i| i + 1).unwrap_or(trimmed.len());
        &trimmed[start..end]
    } else {
        trimmed
    }
}

pub async fn parse_image(
    client: &AnthropicClient,
    image_data: &[u8],
    media_type: &str,
) -> anyhow::Result<ParsedRecipe> {
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, image_data);

    let messages = vec![Message {
        role: "user".into(),
        content: serde_json::json!([
            {
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": media_type,
                    "data": b64,
                }
            },
            {
                "type": "text",
                "text": "Extract the recipe from this image."
            }
        ]),
    }];

    let response = client
        .complete(INGEST_MODEL, INGEST_SYSTEM, messages, 4096)
        .await?;
    let json_str = extract_json(&response);
    let parsed: ParsedRecipe = serde_json::from_str(json_str)?;
    Ok(parsed)
}

pub async fn parse_url(
    client: &AnthropicClient,
    http_client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<ParsedRecipe> {
    let html = http_client.get(url).send().await?.text().await?;

    // Extract readable text in a non-Send block, then own the result
    let text = {
        let document = scraper::Html::parse_document(&html);
        let extracted = ["article", "main", "body"]
            .iter()
            .find_map(|tag| {
                let selector = scraper::Selector::parse(tag).ok()?;
                document
                    .select(&selector)
                    .next()
                    .map(|el| el.text().collect::<Vec<_>>().join(" "))
            })
            .unwrap_or_else(|| document.root_element().text().collect::<Vec<_>>().join(" "));

        // Truncate to ~8000 chars to stay within token limits
        if extracted.len() > 8000 {
            let mut end = 8000;
            while !extracted.is_char_boundary(end) {
                end -= 1;
            }
            extracted[..end].to_string()
        } else {
            extracted
        }
    };

    parse_text(client, &text).await
}
