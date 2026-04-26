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

SECTIONS (parts of a recipe):
- A recipe is composed of one or more sections. A section is a coherent part of the recipe with its own ingredients and steps.
- Examples of real sections: "Těsto", "Náplň", "Drobenka", "Marináda", "Omáčka", "Ozdoba".
- A heading becomes a section ONLY if it has at least one ingredient OR step directly under it. Sub-headings like "Tip", "Poznámka", "Jak podávat", "Varianty", "Podávání" are NOT sections — fold their text into the recipe-level description.
- If the source has no part sub-headings, or only has informational sub-headings, emit exactly ONE section with `label: null`.
- Don't invent sections. Only split when the source explicitly groups ingredients/steps under part headings.
- Conditional headings ("Krém A nebo B", "Volitelná náplň") — emit them as a single section if both alternatives share an ingredient list. If they have separate ingredient lists, emit separate sections. If unclear, default to one section and put the alternatives in the description.
- `step_order` is per-section, starting at 1 for each section.
- Per-section description is optional; only fill if the source has an intro line for that part.
- If the recipe has "assembly" or "finishing" steps that combine components from multiple sections (e.g. "spread filling over dough", "bake at 180 °C", "let cool and serve"), put them in a SEPARATE section. Label it after the dominant action ("Sestavení", "Pečení", "Dokončení") or leave label null if no obvious name fits. Do NOT force recipe-wide steps into the last ingredient section.

TIMES:
- Per-section `prep_time_min` and `cook_time_min` are optional.
- Only fill per-section times when the source EXPLICITLY states a time for that specific part (e.g. "Příprava těsta: 15 min"). Do NOT invent or distribute times to individual sections by guessing.
- If the source gives only an overall recipe time with no per-section breakdown, put it on the section where it most naturally belongs (e.g. put baking time on the baking/finishing section, not on an ingredient-prep section). If unclear, put it on the FIRST section and leave all others null.
- If per-section times are explicit in the source, use them.
- The recipe-level total is computed by the caller as the sum over sections — do NOT emit recipe-level prep_time_min/cook_time_min.

COOK METHOD:
- `cook_method` describes how this section is heat-treated. Set it only on sections that contain actual cooking/baking steps.
- Possible values: "baking" (pečení — oven heat, e.g. "pečte při 180 °C"), "cooking" (vaření — stovetop liquid, e.g. "vařte 10 min"), "frying" (smažení/fritování — pan or deep-fry), "steaming" (dušení/vaření v páře), "other". Null for prep-only sections (mixing, kneading, cutting).
- Infer from the section label or its steps: "Pečení", "bake at 180 °C" → "baking"; "Vaření", "cook until soft" → "cooking".
- A single recipe may have multiple sections with different methods (e.g. "Vaření" + "Pečení").

GUESSED FIELDS:
- "guessed_fields" is an array of field names where you had to guess or infer content that was NOT explicitly in the source.
- Possible values: "description", "servings", "ingredients", "steps", "sections" (if any section was inferred or split), "prep_time_min", "cook_time_min", "tags".
- Only mark fields that required significant guessing. Translating from another language is NOT guessing.

Schema:
{
  "title": string,
  "description": string | null,
  "servings": number | null,
  "tags": [string],
  "sections": [
    {
      "label": string | null,
      "description": string | null,
      "prep_time_min": number | null,
      "cook_time_min": number | null,
      "cook_method": "baking" | "cooking" | "frying" | "steaming" | "other" | null,
      "ingredients": [{ "name": string, "amount": number | null, "unit": string | null, "note": string | null }],
      "steps": [{ "step_order": number, "instruction": string }]
    }
  ],
  "guessed_fields": [string]
}

The `sections` array MUST contain at least one element. For tags: use Czech or common tags. Examples: "rychlý", "vegetariánský", "polévka", "salát", "těstoviny", "česká kuchyně", "dezert", "snídaně", "one-pot", "pečení", "grilování". Assign 1-5 tags."#;

#[derive(Debug, Serialize, Deserialize)]
pub struct ParsedSection {
    pub label: Option<String>,
    pub description: Option<String>,
    pub prep_time_min: Option<i32>,
    pub cook_time_min: Option<i32>,
    #[serde(default)]
    pub cook_method: Option<String>,
    pub ingredients: Vec<IngredientInput>,
    pub steps: Vec<StepInput>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ParsedRecipe {
    pub title: String,
    pub description: Option<String>,
    pub servings: Option<i32>,
    pub tags: Vec<String>,
    pub sections: Vec<ParsedSection>,
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

/// Check if a URL is an Instagram post/reel (not a profile or explore page).
/// Uses exact host matching to prevent SSRF via lookalike domains.
fn is_instagram_url(url_str: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url_str) else {
        return false;
    };
    let is_ig_host = u
        .host_str()
        .map(|h| h == "instagram.com" || h.ends_with(".instagram.com"))
        .unwrap_or(false);
    let path = u.path();
    let is_post_path = path.starts_with("/reel/")
        || path.starts_with("/reels/")
        || path.starts_with("/p/")
        || path.starts_with("/tv/");
    is_ig_host && is_post_path
}

fn extract_instagram_caption(html: &str) -> anyhow::Result<(String, Option<String>)> {
    let document = scraper::Html::parse_document(html);

    let desc_selector = scraper::Selector::parse(r#"meta[name="description"]"#)
        .expect("valid CSS selector: meta[name=description]");
    let caption = document
        .select(&desc_selector)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Nepodařilo se načíst recept z Instagramu. \
                 Zkuste zkopírovat popisek z Instagramu a vložit ho jako text."
            )
        })?;

    // Reject generic Instagram pages (login wall)
    if caption.len() < 20 || caption.starts_with("Instagram") {
        anyhow::bail!(
            "Nepodařilo se načíst recept z Instagramu. \
             Zkuste zkopírovat popisek z Instagramu a vložit ho jako text."
        );
    }

    let og_title_selector = scraper::Selector::parse(r#"meta[property="og:title"]"#)
        .expect("valid CSS selector: meta[property=og:title]");
    let author = document
        .select(&og_title_selector)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| {
            // Handle both Czech "na Instagramu" and English "on Instagram"
            s.split_once(" na Instagramu")
                .or_else(|| s.split_once(" on Instagram"))
                .map_or(s, |(author, _)| author)
        })
        .map(|s| s.trim().to_string());

    Ok((caption, author))
}

/// Parse ISO 8601 duration (e.g. "PT30M", "PT1H30M", "PT45M") to minutes as a string.
fn parse_iso_duration(s: &str) -> Option<String> {
    let s = s.strip_prefix("PT")?;
    let mut total_mins: u32 = 0;

    let mut num_buf = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            num_buf.push(c);
        } else {
            let n: u32 = num_buf.parse().ok()?;
            num_buf.clear();
            match c {
                'H' => total_mins += n * 60,
                'M' => total_mins += n,
                'S' => {} // ignore seconds
                _ => return None,
            }
        }
    }
    if total_mins > 0 {
        Some(total_mins.to_string())
    } else {
        None
    }
}

/// Structured metadata extracted from JSON-LD for supplementing AI output.
struct JsonLdMeta {
    prep_time_min: Option<i32>,
    cook_time_min: Option<i32>,
    servings: Option<i32>,
}

/// Extract machine-readable metadata from JSON-LD (times, servings).
fn extract_jsonld_metadata(document: &scraper::Html) -> Option<JsonLdMeta> {
    let selector = scraper::Selector::parse(r#"script[type="application/ld+json"]"#).ok()?;

    for element in document.select(&selector) {
        let json_text = element.text().collect::<String>();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_text)
            && let Some(meta) = find_recipe_meta(&value)
        {
            return Some(meta);
        }
    }
    None
}

fn find_recipe_meta(value: &serde_json::Value) -> Option<JsonLdMeta> {
    match value {
        serde_json::Value::Object(obj) => {
            let type_field = obj.get("@type").and_then(|v| v.as_str()).unwrap_or("");
            if type_field == "Recipe" {
                let prep = obj
                    .get("prepTime")
                    .and_then(|v| v.as_str())
                    .and_then(parse_iso_duration_mins);
                let cook = obj
                    .get("cookTime")
                    .and_then(|v| v.as_str())
                    .and_then(parse_iso_duration_mins);
                // If no separate prep/cook, try totalTime
                let (prep, cook) = if prep.is_none() && cook.is_none() {
                    let total = obj
                        .get("totalTime")
                        .and_then(|v| v.as_str())
                        .and_then(parse_iso_duration_mins);
                    (total, None)
                } else {
                    (prep, cook)
                };
                let servings = obj.get("recipeYield").and_then(|v| {
                    v.as_str()
                        .and_then(|s| s.split_whitespace().next())
                        .and_then(|s| s.parse::<i32>().ok())
                        .or_else(|| v.as_i64().map(|n| n as i32))
                        .or_else(|| {
                            v.as_array()
                                .and_then(|a| a.first())
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse().ok())
                        })
                });
                return Some(JsonLdMeta {
                    prep_time_min: prep,
                    cook_time_min: cook,
                    servings,
                });
            }
            if let Some(graph) = obj.get("@graph").and_then(|v| v.as_array()) {
                for item in graph {
                    if let Some(meta) = find_recipe_meta(item) {
                        return Some(meta);
                    }
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                if let Some(meta) = find_recipe_meta(item) {
                    return Some(meta);
                }
            }
        }
        _ => {}
    }
    None
}

fn parse_iso_duration_mins(s: &str) -> Option<i32> {
    parse_iso_duration(s).and_then(|s| s.parse().ok())
}

/// Extract recipe data from JSON-LD structured data (schema.org/Recipe).
/// Many recipe sites embed this in <script type="application/ld+json">.
fn extract_jsonld_recipe(document: &scraper::Html) -> Option<String> {
    let selector = scraper::Selector::parse(r#"script[type="application/ld+json"]"#).ok()?;

    for element in document.select(&selector) {
        let json_text = element.text().collect::<String>();
        // Try parsing as a single object or an array
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_text)
            && let Some(recipe_text) = extract_recipe_from_jsonld(&value)
        {
            tracing::debug!("Found JSON-LD Recipe schema ({} chars)", recipe_text.len());
            return Some(recipe_text);
        }
    }
    None
}

fn extract_recipe_from_jsonld(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(obj) => {
            let type_field = obj.get("@type").and_then(|v| v.as_str()).unwrap_or("");
            if type_field == "Recipe" {
                // Convert the structured data to readable text for the AI
                let mut parts = Vec::new();

                if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                    parts.push(format!("Název: {name}"));
                }
                if let Some(desc) = obj.get("description").and_then(|v| v.as_str()) {
                    parts.push(format!("Popis: {desc}"));
                }
                if let Some(prep) = obj.get("prepTime").and_then(|v| v.as_str()) {
                    let mins = parse_iso_duration(prep);
                    parts.push(format!(
                        "Příprava: {} minut",
                        mins.unwrap_or_else(|| prep.to_string())
                    ));
                }
                if let Some(cook) = obj.get("cookTime").and_then(|v| v.as_str()) {
                    let mins = parse_iso_duration(cook);
                    parts.push(format!(
                        "Vaření: {} minut",
                        mins.unwrap_or_else(|| cook.to_string())
                    ));
                }
                if let Some(total) = obj.get("totalTime").and_then(|v| v.as_str()) {
                    let mins = parse_iso_duration(total);
                    parts.push(format!(
                        "Celkový čas: {} minut",
                        mins.unwrap_or_else(|| total.to_string())
                    ));
                }
                if let Some(servings) = obj.get("recipeYield").and_then(|v| {
                    v.as_str().map(|s| s.to_string()).or_else(|| {
                        v.as_array()
                            .and_then(|a| a.first())
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    })
                }) {
                    parts.push(format!("Porce: {servings}"));
                }

                if let Some(ingredients) = obj.get("recipeIngredient").and_then(|v| v.as_array()) {
                    let ings: Vec<&str> = ingredients.iter().filter_map(|v| v.as_str()).collect();
                    if !ings.is_empty() {
                        parts.push(format!("Ingredience:\n{}", ings.join("\n")));
                    }
                }

                if let Some(instructions) = obj.get("recipeInstructions").and_then(|v| v.as_array())
                {
                    let steps: Vec<String> = instructions
                        .iter()
                        .enumerate()
                        .filter_map(|(i, v)| {
                            v.as_str()
                                .map(|s| s.to_string())
                                .or_else(|| {
                                    v.get("text")
                                        .and_then(|t| t.as_str())
                                        .map(|s| s.to_string())
                                })
                                .map(|s| format!("{}. {s}", i + 1))
                        })
                        .collect();
                    if !steps.is_empty() {
                        parts.push(format!("Postup:\n{}", steps.join("\n")));
                    }
                }

                return Some(parts.join("\n\n"));
            }

            // Check for @graph array (some sites wrap recipes in @graph)
            if let Some(graph) = obj.get("@graph").and_then(|v| v.as_array()) {
                for item in graph {
                    if let Some(text) = extract_recipe_from_jsonld(item) {
                        return Some(text);
                    }
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                if let Some(text) = extract_recipe_from_jsonld(item) {
                    return Some(text);
                }
            }
        }
        _ => {}
    }
    None
}

pub async fn parse_url(
    client: &AnthropicClient,
    http_client: &reqwest::Client,
    browser: Option<&chromiumoxide::Browser>,
    url: &str,
) -> anyhow::Result<ParsedRecipe> {
    let wait_condition = crate::scraper::browser_wait_condition(url);
    let (html, final_url) = if wait_condition.is_some() {
        if let Some(browser) = browser {
            let wait = wait_condition.unwrap_or(crate::browser::WaitCondition::NetworkIdle);
            let html =
                crate::browser::fetch_html(browser, url, &wait, std::time::Duration::from_secs(30))
                    .await?;
            (html, url.to_string())
        } else {
            // No browser available -- fall back to reqwest (may fail)
            let response = http_client.get(url).send().await?;
            let final_url = response.url().to_string();
            let html = response.text().await?;
            (html, final_url)
        }
    } else {
        let response = http_client.get(url).send().await?;
        let final_url = response.url().to_string();
        let html = response.text().await?;
        (html, final_url)
    };

    // Check final URL (after redirects) for Instagram — handles ig.me short links
    if is_instagram_url(url) || is_instagram_url(&final_url) {
        // Detect login redirect
        if final_url.contains("/accounts/login") {
            tracing::warn!("Instagram redirected to login for URL: {url}");
            anyhow::bail!(
                "Nepodařilo se načíst recept z Instagramu. \
                 Zkuste zkopírovat popisek z Instagramu a vložit ho jako text."
            );
        }

        let (caption, author) = extract_instagram_caption(&html).inspect_err(|e| {
            tracing::warn!("Instagram caption extraction failed for {url}: {e}");
        })?;
        tracing::debug!(
            "Instagram extraction for {url}: caption={} chars, author={:?}",
            caption.len(),
            author
        );
        let text = match &author {
            Some(a) => format!("Source: Instagram post by {a}\nURL: {url}\n\nCaption:\n{caption}"),
            None => format!("Source: Instagram post\nURL: {url}\n\nCaption:\n{caption}"),
        };
        return parse_text(client, &text).await;
    }

    // Non-Instagram: try JSON-LD structured data first, then fall back to HTML text.
    // Scoped to drop the scraper::Html before the async parse_text call
    // (scraper::Html uses tendril::NonAtomic which is !Send).
    let (text, jsonld_meta) = {
        let document = scraper::Html::parse_document(&html);
        let meta = extract_jsonld_metadata(&document);
        let text = extract_jsonld_recipe(&document).unwrap_or_else(|| {
            // Fallback: extract readable text from article/main/body
            ["article", "main", "body"]
                .iter()
                .find_map(|tag| {
                    let selector = scraper::Selector::parse(tag).ok()?;
                    document
                        .select(&selector)
                        .next()
                        .map(|el| el.text().collect::<Vec<_>>().join(" "))
                })
                .unwrap_or_else(|| document.root_element().text().collect::<Vec<_>>().join(" "))
        });
        (text, meta)
    };

    // Truncate to ~8000 chars to stay within token limits
    let text = if text.len() > 8000 {
        let mut end = 8000;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text[..end].to_string()
    } else {
        text
    };

    let mut recipe = parse_text(client, &text).await?;

    // Supplement AI output with structured data from JSON-LD (more reliable than AI extraction)
    // Times go on the first section (per spec: single total goes on first section).
    if let Some(meta) = jsonld_meta {
        if let Some(first_section) = recipe.sections.first_mut() {
            if first_section.prep_time_min.is_none() {
                first_section.prep_time_min = meta.prep_time_min;
            }
            if first_section.cook_time_min.is_none() {
                first_section.cook_time_min = meta.cook_time_min;
            }
        }
        if recipe.servings.is_none() {
            recipe.servings = meta.servings;
        }
    }

    Ok(recipe)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- is_instagram_url --

    #[test]
    fn instagram_reel_url() {
        assert!(is_instagram_url(
            "https://www.instagram.com/reels/DCwqunhI7l6/"
        ));
    }

    #[test]
    fn instagram_reel_singular() {
        assert!(is_instagram_url("https://www.instagram.com/reel/ABC123/"));
    }

    #[test]
    fn instagram_post_url() {
        assert!(is_instagram_url("https://www.instagram.com/p/XYZ789/"));
    }

    #[test]
    fn instagram_tv_url() {
        assert!(is_instagram_url("https://www.instagram.com/tv/ABC123/"));
    }

    #[test]
    fn instagram_mobile_subdomain() {
        assert!(is_instagram_url("https://m.instagram.com/reel/ABC123/"));
    }

    #[test]
    fn instagram_bare_domain() {
        assert!(is_instagram_url("https://instagram.com/p/ABC123/"));
    }

    #[test]
    fn instagram_profile_rejected() {
        assert!(!is_instagram_url("https://www.instagram.com/someuser/"));
    }

    #[test]
    fn instagram_explore_rejected() {
        assert!(!is_instagram_url(
            "https://www.instagram.com/explore/tags/food/"
        ));
    }

    #[test]
    fn lookalike_domain_rejected() {
        assert!(!is_instagram_url("https://evilinstagram.com/reel/ABC123/"));
    }

    #[test]
    fn ssrf_subdomain_rejected() {
        assert!(!is_instagram_url(
            "https://instagram.com.evil.local/reel/ABC123/"
        ));
    }

    #[test]
    fn non_instagram_url() {
        assert!(!is_instagram_url("https://google.com/search?q=recipe"));
    }

    #[test]
    fn invalid_url() {
        assert!(!is_instagram_url("not a url at all"));
    }

    // -- extract_instagram_caption --

    #[test]
    fn extracts_caption_and_author_czech() {
        let html = r#"<html><head>
            <meta name="description" content="100 likes - Tagesrezept: Leckere Hähnchen-Pfanne" />
            <meta property="og:title" content="Tagesrezept na Instagramu: Leckere Hähnchen" />
        </head><body></body></html>"#;

        let (caption, author) = extract_instagram_caption(html).unwrap();
        assert_eq!(caption, "100 likes - Tagesrezept: Leckere Hähnchen-Pfanne");
        assert_eq!(author.as_deref(), Some("Tagesrezept"));
    }

    #[test]
    fn extracts_author_english() {
        let html = r#"<html><head>
            <meta name="description" content="A delicious chicken recipe with rice and cream" />
            <meta property="og:title" content="FoodBlog on Instagram: Chicken recipe" />
        </head><body></body></html>"#;

        let (_, author) = extract_instagram_caption(html).unwrap();
        assert_eq!(author.as_deref(), Some("FoodBlog"));
    }

    #[test]
    fn rejects_empty_caption() {
        let html = r#"<html><head>
            <meta name="description" content="" />
        </head><body></body></html>"#;

        assert!(extract_instagram_caption(html).is_err());
    }

    #[test]
    fn rejects_short_caption() {
        let html = r#"<html><head>
            <meta name="description" content="Short" />
        </head><body></body></html>"#;

        assert!(extract_instagram_caption(html).is_err());
    }

    #[test]
    fn rejects_generic_instagram_page() {
        let html = r#"<html><head>
            <meta name="description" content="Instagram - photos and videos from friends" />
        </head><body></body></html>"#;

        assert!(extract_instagram_caption(html).is_err());
    }

    #[test]
    fn rejects_missing_meta_description() {
        let html = r#"<html><head>
            <meta name="viewport" content="width=device-width" />
        </head><body></body></html>"#;

        assert!(extract_instagram_caption(html).is_err());
    }

    #[test]
    fn handles_no_og_title() {
        let html = r#"<html><head>
            <meta name="description" content="A long enough caption with recipe ingredients listed here" />
        </head><body></body></html>"#;

        let (caption, author) = extract_instagram_caption(html).unwrap();
        assert!(caption.contains("recipe ingredients"));
        assert!(author.is_none());
    }

    #[test]
    fn html_entities_decoded() {
        let html = r#"<html><head>
            <meta name="description" content="Leckere H&#xe4;hnchen-Pfanne mit Reis &amp; Gem&#xfc;se" />
        </head><body></body></html>"#;

        let (caption, _) = extract_instagram_caption(html).unwrap();
        assert!(caption.contains("Hähnchen"));
        assert!(caption.contains("&"));
        assert!(caption.contains("Gemüse"));
    }

    // -- parse_iso_duration --

    #[test]
    fn iso_duration_30_minutes() {
        assert_eq!(parse_iso_duration("PT30M"), Some("30".to_string()));
    }

    #[test]
    fn iso_duration_1h30m() {
        assert_eq!(parse_iso_duration("PT1H30M"), Some("90".to_string()));
    }

    #[test]
    fn iso_duration_1h() {
        assert_eq!(parse_iso_duration("PT1H"), Some("60".to_string()));
    }

    #[test]
    fn iso_duration_45m() {
        assert_eq!(parse_iso_duration("PT45M"), Some("45".to_string()));
    }

    #[test]
    fn iso_duration_2h15m() {
        assert_eq!(parse_iso_duration("PT2H15M"), Some("135".to_string()));
    }

    #[test]
    fn iso_duration_zero_returns_none() {
        assert_eq!(parse_iso_duration("PT0M"), None);
    }

    #[test]
    fn iso_duration_missing_pt_prefix() {
        assert_eq!(parse_iso_duration("30M"), None);
    }

    #[test]
    fn iso_duration_invalid_string() {
        assert_eq!(parse_iso_duration("invalid"), None);
    }

    // -- extract_jsonld_recipe & extract_jsonld_metadata --

    #[test]
    fn jsonld_basic_recipe_schema() {
        let html = r#"<html><head>
            <script type="application/ld+json">{"@type":"Recipe","name":"Palačinky","prepTime":"PT20M","cookTime":"PT15M","recipeYield":"4 porce","recipeIngredient":["mouka","vejce","mléko"],"recipeInstructions":[{"@type":"HowToStep","text":"Smíchejte ingredience."},{"@type":"HowToStep","text":"Pečte na pánvi."}]}</script>
        </head><body></body></html>"#;

        let document = scraper::Html::parse_document(html);

        let text = extract_jsonld_recipe(&document).expect("should extract recipe");
        assert!(
            text.contains("Název: Palačinky"),
            "expected 'Název: Palačinky' in: {text}"
        );
        assert!(text.contains("mouka"), "expected 'mouka' in: {text}");
        assert!(
            text.contains("Smíchejte ingredience"),
            "expected step text in: {text}"
        );

        let meta = extract_jsonld_metadata(&document).expect("should extract metadata");
        assert_eq!(meta.prep_time_min, Some(20));
        assert_eq!(meta.cook_time_min, Some(15));
        assert_eq!(meta.servings, Some(4));
    }

    #[test]
    fn jsonld_recipe_inside_graph_array() {
        let html = r#"<html><head>
            <script type="application/ld+json">{"@graph":[{"@type":"WebPage"},{"@type":"Recipe","name":"Guláš","prepTime":"PT30M","cookTime":"PT2H","recipeYield":"6"}]}</script>
        </head><body></body></html>"#;

        let document = scraper::Html::parse_document(html);

        let text = extract_jsonld_recipe(&document).expect("should find Recipe inside @graph");
        assert!(
            text.contains("Název: Guláš"),
            "expected 'Název: Guláš' in: {text}"
        );

        let meta = extract_jsonld_metadata(&document).expect("should extract metadata from @graph");
        assert_eq!(meta.prep_time_min, Some(30));
        assert_eq!(meta.cook_time_min, Some(120));
        assert_eq!(meta.servings, Some(6));
    }

    #[test]
    fn jsonld_no_recipe_schema() {
        let html = r#"<html><head>
            <script type="application/ld+json">{"@type":"WebPage","name":"Homepage"}</script>
        </head><body></body></html>"#;

        let document = scraper::Html::parse_document(html);
        assert!(extract_jsonld_recipe(&document).is_none());
        assert!(extract_jsonld_metadata(&document).is_none());
    }

    #[test]
    fn jsonld_recipe_yield_as_number() {
        let html = r#"<html><head>
            <script type="application/ld+json">{"@type":"Recipe","name":"Test","recipeYield":4}</script>
        </head><body></body></html>"#;

        let document = scraper::Html::parse_document(html);

        let meta = extract_jsonld_metadata(&document).expect("should extract metadata");
        assert_eq!(meta.servings, Some(4));
    }
}
