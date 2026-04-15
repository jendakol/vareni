//! AI scoring for recipe discovery candidates.
//! Single Haiku call per candidate: canonical name, restriction check, duplicate check, relevance score.

use crate::ai::client::{AnthropicClient, Message};
use serde::{Deserialize, Serialize};

const DISCOVERY_MODEL: &str = "claude-haiku-4-5-20251001";

#[derive(Debug, Serialize, Deserialize)]
pub struct ScoringResult {
    pub canonical_name: String,
    pub violates_restriction: bool,
    pub restriction_violated: Option<String>,
    pub is_duplicate: bool,
    pub duplicate_of: Option<String>,
    pub relevance_score: f32,
}

/// Score a recipe candidate against user preferences, restrictions, and existing recipes.
#[allow(clippy::too_many_arguments)]
pub async fn score_candidate(
    client: &AnthropicClient,
    title: &str,
    description: Option<&str>,
    ingredients: &[String],
    tags: &[String],
    user_prompt: Option<&str>,
    preferences_json: &str,
    restrictions_json: &str,
    existing_recipes: &str,
    rejected_recipes: &str,
) -> anyhow::Result<ScoringResult> {
    let desc = description.unwrap_or("(bez popisu)");
    let ings = ingredients.join(", ");
    let tags_str = tags.join(", ");
    let user_query = user_prompt.unwrap_or("(žádný konkrétní dotaz)");

    let system = format!(
        "You are evaluating a recipe candidate for a household meal planning app.\n\n\
         Candidate recipe:\n\
         Title: {title}\n\
         Description: {desc}\n\
         Ingredients: {ings}\n\
         Tags: {tags_str}\n\n\
         User searched for: {user_query}\n\
         User's food PREFERENCES (use ONLY for scoring, NOT for rejecting): {preferences_json}\n\
         User's dietary RESTRICTIONS (use ONLY for rejection check): {restrictions_json}\n\n\
         Existing recipes in the book: {existing_recipes}\n\
         Previously rejected-similar recipes: {rejected_recipes}\n\n\
         Tasks:\n\
         1. CANONICAL NAME: Reduce the recipe title to a short canonical Czech dish name \
            (e.g. \"kuře na paprice\", \"mac and cheese\", \"špenátový salát s fetou\"). Use proper Czech diacritics.\n\
         2. RESTRICTION CHECK: Does this recipe contain a restricted ingredient from the list above? \
            ONLY check against the EXACT restrictions provided. Do NOT invent or infer additional restrictions. \
            If the restriction list says \"kopr\" — only reject if the recipe contains dill. \
            If the restriction list is empty — do NOT reject anything. \
            Be conservative: only reject if the restricted ingredient is a KEY component, \
            not a minor/optional/substitutable ingredient. When in doubt, do NOT reject.\n\
         3. DUPLICATE CHECK: Is this essentially the same dish as any existing recipe? \
            Consider the canonical name and ingredients, not just the title. (yes/no, which existing recipe)\n\
         4. RELEVANCE SCORE: Rate 0.0-1.0 how well this recipe matches what the user is looking for. \
            Consider BOTH the user's search query AND their food preferences. \
            If the user searched for \"rychlá večeře\" and the recipe takes 100 minutes, score LOW. \
            If no preferences are set and no search query, score 0.5 for any reasonable recipe.\n\n\
         Return ONLY valid JSON:\n\
         {{\"canonical_name\": \"string\", \"violates_restriction\": false, \
         \"restriction_violated\": null, \"is_duplicate\": false, \
         \"duplicate_of\": null, \"relevance_score\": 0.8}}"
    );

    let messages = vec![Message {
        role: "user".into(),
        content: serde_json::json!(format!("Evaluate this recipe: {title}")),
    }];

    let response = client
        .complete(DISCOVERY_MODEL, &system, messages, 1024)
        .await?;
    let json_str = extract_json(&response);
    let result: ScoringResult = serde_json::from_str(json_str).map_err(|e| {
        tracing::error!("Failed to parse discovery scoring response: {e}\nRaw: {response}");
        e
    })?;

    Ok(result)
}

/// Translate a Czech search query to a target language for foreign recipe sites.
/// Uses a fast Haiku call. Returns keywords suitable for a search engine.
pub async fn translate_query(
    client: &AnthropicClient,
    czech_query: &str,
    target_language: &str,
) -> anyhow::Result<String> {
    let system = format!(
        "Translate the following Czech recipe search query to {target_language}. \
         Return ONLY the translated keywords, nothing else. No quotes, no explanation. \
         Keep it short and suitable for a recipe search engine."
    );

    let messages = vec![Message {
        role: "user".into(),
        content: serde_json::json!(czech_query),
    }];

    let response = client
        .complete(DISCOVERY_MODEL, &system, messages, 100)
        .await?;

    Ok(response.trim().to_string())
}

/// Strip markdown code fences and find the JSON object.
fn extract_json(response: &str) -> &str {
    let trimmed = response.trim();
    if let Some(start) = trimmed.find('{') {
        let end = trimmed.rfind('}').map(|i| i + 1).unwrap_or(trimmed.len());
        &trimmed[start..end]
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_from_fenced_response() {
        let input = "```json\n{\"canonical_name\": \"test\"}\n```";
        assert_eq!(extract_json(input), "{\"canonical_name\": \"test\"}");
    }

    #[test]
    fn extract_json_from_bare_response() {
        let input = "{\"canonical_name\": \"test\"}";
        assert_eq!(extract_json(input), "{\"canonical_name\": \"test\"}");
    }

    #[test]
    fn extract_json_with_preamble() {
        let input = "Here is the JSON:\n{\"canonical_name\": \"test\"}";
        assert_eq!(extract_json(input), "{\"canonical_name\": \"test\"}");
    }
}
