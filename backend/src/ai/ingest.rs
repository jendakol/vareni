use crate::ai::client::{AnthropicClient, Message};
use crate::models::{IngredientInput, StepInput};
use serde::{Deserialize, Serialize};

const INGEST_MODEL: &str = "claude-sonnet-4-6";
const INGEST_SYSTEM: &str = r#"You are a recipe parser for a Czech cooking app. ALL output MUST be in Czech language.
Extract the recipe from the user's input and return ONLY valid JSON. No preamble, no markdown, no explanation.

IMPORTANT RULES:
- Title, description, ingredient names, steps — everything MUST be in Czech. Translate if the source is in another language.
- Ingredient names must be specific Czech grocery terms (e.g. "cibule" not "onion", "máslo" not "butter").
- Always try to extract amounts and units from the recipe. Use Czech units: g, kg, ml, l, lžíce, lžička, ks, hrnek, podle chuti.
- If the source is a photo of a recipe (handwritten, printed, screenshot), do your best to read all text including amounts.
- If the recipe is incomplete or unclear, fill in reasonable defaults based on your cooking knowledge — but mark them as guessed.
- Write a short, appetizing Czech description (1-2 sentences) even if the source doesn't have one.
- Estimate prep_time_min and cook_time_min if not explicitly stated.

GUESSED FIELDS:
- "guessed_fields" is an array of field names where you had to guess or infer content that was NOT explicitly in the source.
- For example, if the source only had ingredients but no steps, include "steps" in guessed_fields.
- If you estimated servings, include "servings". If you wrote the description yourself, include "description".
- If individual steps were guessed, include "steps". If individual ingredients were guessed, include "ingredients".
- Only mark fields that required significant guessing. Translating from another language is NOT guessing.

Schema:
{
  "title": string,
  "description": string | null,
  "servings": number | null,
  "prep_time_min": number | null,
  "cook_time_min": number | null,
  "tags": [string],
  "ingredients": [{ "name": string, "amount": number | null, "unit": string | null, "note": string | null }],
  "steps": [{ "step_order": number, "instruction": string }],
  "guessed_fields": [string]
}

For tags: use Czech or common tags. Examples: "rychlý", "vegetariánský", "polévka", "salát",
"těstoviny", "česká kuchyně", "dezert", "snídaně", "one-pot", "pečení", "grilování".
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
    #[serde(default)]
    pub guessed_fields: Vec<String>,
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
    parse_images(client, &[(image_data, media_type)]).await
}

pub async fn parse_images(
    client: &AnthropicClient,
    images: &[(&[u8], &str)],
) -> anyhow::Result<ParsedRecipe> {
    let mut content = Vec::new();
    for (data, media_type) in images {
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data);
        content.push(serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": media_type,
                "data": b64,
            }
        }));
    }
    content.push(serde_json::json!({
        "type": "text",
        "text": "Extract the recipe from these images. They are all part of the same recipe."
    }));

    let messages = vec![Message {
        role: "user".into(),
        content: serde_json::Value::Array(content),
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
