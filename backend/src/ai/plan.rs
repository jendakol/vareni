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

pub async fn suggest(
    client: &AnthropicClient,
    history_json: &str,
    restrictions_json: &str,
    recipes_json: &str,
    prompt: &str,
) -> anyhow::Result<Vec<SuggestedEntry>> {
    let system = format!(
        "You are a meal planning assistant. Suggest meals for the upcoming days.\n\
         Avoid repeating meals from recent history. Respect all dietary restrictions.\n\
         Prefer variety in tags — don't suggest three soups in a row.\n\n\
         Recent history (last 90 days):\n<history>{history_json}</history>\n\n\
         Dietary restrictions:\n<restrictions>{restrictions_json}</restrictions>\n\n\
         Available recipes (with tags):\n<recipes>{recipes_json}</recipes>\n\n\
         User request: {prompt}\n\n\
         Return ONLY a valid JSON array:\n\
         [{{\"date\": \"YYYY-MM-DD\", \"meal_type\": \"lunch|dinner\", \
         \"recipe_id\": \"uuid or null\", \"free_text\": \"string or null\", \
         \"note\": \"string or null\"}}]"
    );

    let messages = vec![Message {
        role: "user".into(),
        content: serde_json::json!(prompt),
    }];

    let response = client.complete(PLAN_MODEL, &system, messages, 4096).await?;
    let suggestions: Vec<SuggestedEntry> = serde_json::from_str(&response)?;
    Ok(suggestions)
}
