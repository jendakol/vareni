use crate::ai::client::{AnthropicClient, Message};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const PLAN_MODEL: &str = "claude-haiku-4-5-20251001";

#[derive(Debug, Serialize, Deserialize)]
pub struct SuggestedEntry {
    pub date: String,
    pub meal_type: String,
    pub recipe_id: Option<Uuid>,
    pub free_text: Option<String>,
    pub note: Option<String>,
}

/// Strip markdown code fences and find the JSON array in Claude's response.
fn extract_json_array(response: &str) -> &str {
    let trimmed = response.trim();
    if let Some(start) = trimmed.find('[') {
        let end = trimmed.rfind(']').map(|i| i + 1).unwrap_or(trimmed.len());
        &trimmed[start..end]
    } else {
        trimmed
    }
}

pub async fn suggest(
    client: &AnthropicClient,
    history_json: &str,
    restrictions_json: &str,
    preferences_json: &str,
    recipes_json: &str,
    prompt: &str,
) -> anyhow::Result<Vec<SuggestedEntry>> {
    let system = format!(
        "You are a meal planning assistant. Suggest meals for the upcoming days.\n\
         Avoid repeating meals from recent history. Respect all dietary restrictions. \
         Favor meals matching the user's food preferences.\n\
         Prefer variety in tags — don't suggest three soups in a row.\n\n\
         Recent history (last 90 days):\n<history>{history_json}</history>\n\n\
         Dietary restrictions:\n<restrictions>{restrictions_json}</restrictions>\n\n\
         Food preferences (favor these):\n<preferences>{preferences_json}</preferences>\n\n\
         Available recipes (with tags):\n<recipes>{recipes_json}</recipes>\n\n\
         User request: {prompt}\n\n\
         Return ONLY a valid JSON array:\n\
         [{{\"date\": \"YYYY-MM-DD\", \"meal_type\": \"lunch|dinner\", \
         \"recipe_id\": \"uuid or null\", \"free_text\": \"string or null\", \
         \"note\": \"string or null\"}}]\n\n\
         IMPORTANT RULES:\n\
         - ONLY suggest recipes from the available recipes list above. Do NOT invent meals.\n\
         - Always set recipe_id to the recipe's UUID and free_text to its title.\n\
         - If the recipe list is too small for the requested period, repeat recipes \
         (but space them out) rather than inventing new ones."
    );

    let messages = vec![Message {
        role: "user".into(),
        content: serde_json::json!(prompt),
    }];

    let response = client.complete(PLAN_MODEL, &system, messages, 4096).await?;
    let json_str = extract_json_array(&response);
    let suggestions: Vec<SuggestedEntry> = serde_json::from_str(json_str)
        .map_err(|e| {
            tracing::error!("Failed to parse plan AI response: {e}\nRaw response: {response}");
            e
        })?;
    Ok(suggestions)
}
