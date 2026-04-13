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

pub async fn parse_url(
    client: &AnthropicClient,
    http_client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<ParsedRecipe> {
    let response = http_client.get(url).send().await?;
    let final_url = response.url().to_string();
    let html = response.text().await?;

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

    // Non-Instagram: extract readable text
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
}
