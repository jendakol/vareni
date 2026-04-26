//! Discovery endpoint: orchestrates scrape -> parse -> embed -> score -> insert.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use crate::AppState;
use crate::ai::client::AnthropicClient;
use crate::auth::AuthUser;
use crate::embedding::EmbeddingService;
use crate::error::{AppError, AppResult};
use crate::models::{DiscoverRequest, DiscoverResponse, SiteError, SkippedCounts};
use crate::{db, scraper};

pub async fn discover(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<DiscoverRequest>,
) -> AppResult<Json<DiscoverResponse>> {
    let embedding_svc = state.embedding.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable(
            "Discovery is unavailable: embedding model not configured".into(),
        )
    })?;

    let count = body.count.unwrap_or(5).min(10);
    let planning_for = body.planning_for.as_deref().unwrap_or("both");

    let client = AnthropicClient::new(&state.config.anthropic_api_key);

    // Gather user context
    let restrictions = if planning_for == "me" {
        db::users::get_dietary_restrictions(&state.pool, auth.user_id)
            .await
            .map_err(AppError::Sqlx)?
    } else {
        db::users::get_all_dietary_restrictions(&state.pool)
            .await
            .map_err(AppError::Sqlx)?
    };
    let preferences = if planning_for == "me" {
        db::users::get_food_preferences(&state.pool, auth.user_id)
            .await
            .map_err(AppError::Sqlx)?
    } else {
        db::users::get_all_food_preferences(&state.pool)
            .await
            .map_err(AppError::Sqlx)?
    };

    let restrictions_json = serde_json::to_string(&restrictions).unwrap_or_default();
    let preferences_json = serde_json::to_string(&preferences).unwrap_or_default();

    // Get existing recipe titles for dedup context
    let existing_statuses = &["saved", "tested"];
    let (existing_recipes, _) = db::recipes::list(
        &state.pool,
        None,
        None,
        "recent",
        1,
        1000,
        existing_statuses,
    )
    .await?;

    let existing_titles: Vec<String> = existing_recipes
        .iter()
        .map(|r| {
            if let Some(ref cn) = r.canonical_name {
                format!("{} ({})", r.title, cn)
            } else {
                r.title.clone()
            }
        })
        .collect();
    let existing_titles_str = existing_titles.join(", ");

    // Get rejected-similar recipes for auto-filtering
    let rejected_statuses = &["rejected_similar"];
    let (rejected_recipes, _) = db::recipes::list(
        &state.pool,
        None,
        None,
        "recent",
        1,
        1000,
        rejected_statuses,
    )
    .await?;
    let rejected_titles: Vec<String> = rejected_recipes.iter().map(|r| r.title.clone()).collect();
    let rejected_titles_str = rejected_titles.join(", ");

    // Scrape recipe URLs from all curated sites.
    // Fetch more URLs than requested to account for filtering losses (duplicates, restrictions).
    let providers = scraper::providers();
    let fetch_multiplier = 3; // process 3x more URLs than desired results
    let urls_per_site = ((count * fetch_multiplier) / providers.len()).max(3);

    // Translate the user's Czech prompt for foreign-language providers (one Haiku call per language)
    let mut translated_prompts: HashMap<&str, Option<String>> = HashMap::new();
    translated_prompts.insert("cs", body.prompt.clone());

    if body.prompt.is_some() {
        let languages: Vec<&str> = providers
            .iter()
            .map(|p| p.language())
            .filter(|l| *l != "cs")
            .collect();
        for lang in languages {
            if translated_prompts.contains_key(lang) {
                continue;
            }
            match crate::ai::discovery::translate_query(
                &client,
                body.prompt.as_deref().unwrap(),
                lang,
            )
            .await
            {
                Ok(translated) => {
                    tracing::info!(from = "cs", to = lang, original = ?body.prompt, translated = %translated, "Translated search query");
                    translated_prompts.insert(lang, Some(translated));
                }
                Err(e) => {
                    tracing::warn!(lang = lang, error = %e, "Failed to translate query, skipping language");
                    translated_prompts.insert(lang, None);
                }
            }
        }
    }

    // Launch headless browser if any provider requires it
    let any_needs_browser = providers.iter().any(|p| p.requires_browser());
    let browser_guard;
    let browser_handle;
    let browser: Option<chromiumoxide::Browser> = if any_needs_browser {
        match state.browser_semaphore.clone().acquire_owned().await {
            Ok(permit) => {
                browser_guard = Some(permit);
                match crate::browser::launch().await {
                    Ok((b, h)) => {
                        browser_handle = Some(h);
                        Some(b)
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to launch browser — browser-requiring providers will be skipped");
                        browser_handle = None;
                        None
                    }
                }
            }
            Err(_) => {
                tracing::warn!("Browser semaphore closed");
                browser_guard = None;
                browser_handle = None;
                None
            }
        }
    } else {
        browser_guard = None;
        browser_handle = None;
        None
    };
    // Suppress unused variable warnings when browser isn't launched
    let _ = &browser_guard;
    let _ = &browser_handle;

    let mut all_urls: Vec<String> = Vec::new();
    let mut errors: Vec<SiteError> = Vec::new();

    for provider in &providers {
        let prompt = translated_prompts
            .get(provider.language())
            .and_then(|p| p.as_deref());

        match scraper::fetch_recipe_urls(
            &state.http_client,
            browser.as_ref(),
            provider.as_ref(),
            prompt,
            urls_per_site,
        )
        .await
        {
            Ok(urls) => {
                tracing::info!(
                    site = provider.name(),
                    url_count = urls.len(),
                    "Scraped recipe URLs"
                );
                all_urls.extend(urls);
            }
            Err(e) => {
                tracing::warn!(site = provider.name(), error = %e, "Site scraping failed");
                errors.push(SiteError {
                    site: provider.name().to_string(),
                    error: e,
                });
            }
        }
        // Rate limit: 500ms between sites
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    // Shuffle to mix sources — otherwise Czech reqwest sites always get priority
    {
        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        all_urls.shuffle(&mut rng);
    }

    // Cap total URLs to process (more than requested, to survive filtering)
    let max_process = (count * fetch_multiplier).min(30);
    all_urls.truncate(max_process);

    // Process each candidate (with rate limiting between fetches).
    // Stop early once we have enough discovered recipes.
    let mut discovered = Vec::new();
    let mut skipped = SkippedCounts::default();

    for (i, url) in all_urls.iter().enumerate() {
        // Stop early if we have enough results
        if discovered.len() >= count {
            break;
        }

        // Rate limit: 500ms between individual recipe page fetches (spec section 3)
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        // URL dedup: skip if we already have a recipe from this URL
        let url_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM recipes WHERE source_url = $1)",
        )
        .bind(url)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);

        if url_exists {
            skipped.duplicate += 1;
            continue;
        }

        let result = process_candidate(
            &state,
            embedding_svc,
            &client,
            browser.as_ref(),
            auth.user_id,
            url,
            body.prompt.as_deref(),
            &restrictions_json,
            &preferences_json,
            &existing_titles_str,
            &rejected_titles_str,
        )
        .await;

        match result {
            Ok(CandidateResult::Discovered(recipe)) => discovered.push(*recipe),
            Ok(CandidateResult::Duplicate) => skipped.duplicate += 1,
            Ok(CandidateResult::Restricted) => skipped.restricted += 1,
            Ok(CandidateResult::LowScore) => skipped.low_score += 1,
            Ok(CandidateResult::SimilarToRejected) => skipped.similar_to_rejected += 1,
            Err(e) => {
                tracing::warn!(url = %url, error = %e, "Failed to process candidate");
                skipped.failed += 1;
            }
        }
    }

    Ok(Json(DiscoverResponse {
        discovered,
        skipped,
        errors,
    }))
}

enum CandidateResult {
    Discovered(Box<crate::models::Recipe>),
    Duplicate,
    Restricted,
    LowScore,
    SimilarToRejected,
}

#[allow(clippy::too_many_arguments)]
async fn process_candidate(
    state: &AppState,
    embedding_svc: &Arc<EmbeddingService>,
    client: &AnthropicClient,
    browser: Option<&chromiumoxide::Browser>,
    owner_id: uuid::Uuid,
    url: &str,
    user_prompt: Option<&str>,
    restrictions_json: &str,
    preferences_json: &str,
    existing_titles: &str,
    rejected_titles: &str,
) -> anyhow::Result<CandidateResult> {
    // Step 1: Parse the recipe from URL (reuse existing ingestion)
    tracing::info!(url = %url, "Parsing recipe candidate");
    let parsed = crate::ai::ingest::parse_url(client, &state.http_client, browser, url).await?;

    // Flatten ingredients from all sections for embedding/scoring purposes
    let ingredient_names: Vec<String> = parsed
        .sections
        .iter()
        .flat_map(|s| s.ingredients.iter().map(|i| i.name.clone()))
        .collect();
    let tags: Vec<String> = parsed.tags.clone();

    // Step 2: Mechanical embedding pre-filter
    let mech_text =
        EmbeddingService::recipe_summary_mechanical(&parsed.title, &tags, &ingredient_names);
    let mech_embedding = embedding_svc
        .embed(&mech_text)
        .ok_or_else(|| anyhow::anyhow!("Failed to compute embedding"))?;

    // Check against rejected_similar (threshold 0.70)
    let rejected_similar =
        db::recipes::find_similar(&state.pool, &mech_embedding, &["rejected_similar"], 3).await?;

    if let Some((_, _, _, sim)) = rejected_similar.first()
        && *sim > 0.70
    {
        tracing::debug!(url = %url, similarity = %sim, "Skipped: similar to rejected");
        return Ok(CandidateResult::SimilarToRejected);
    }

    // Check against existing recipes (threshold 0.90 for auto-skip)
    let existing_similar = db::recipes::find_similar(
        &state.pool,
        &mech_embedding,
        &["saved", "tested", "discovered"],
        3,
    )
    .await?;

    if let Some((_, _, _, sim)) = existing_similar.first()
        && *sim > 0.90
    {
        tracing::debug!(url = %url, similarity = %sim, "Skipped: near-duplicate of existing");
        return Ok(CandidateResult::Duplicate);
    }

    // Step 3: AI scoring call
    tracing::info!(url = %url, title = %parsed.title, "Scoring candidate with AI");
    let score = crate::ai::discovery::score_candidate(
        client,
        &parsed.title,
        parsed.description.as_deref(),
        &ingredient_names,
        &tags,
        user_prompt,
        preferences_json,
        restrictions_json,
        existing_titles,
        rejected_titles,
    )
    .await?;

    if score.violates_restriction {
        tracing::info!(
            url = %url,
            restriction = ?score.restriction_violated,
            "Skipped: violates restriction"
        );
        return Ok(CandidateResult::Restricted);
    }
    if score.is_duplicate {
        tracing::info!(
            url = %url,
            duplicate_of = ?score.duplicate_of,
            "Skipped: duplicate"
        );
        return Ok(CandidateResult::Duplicate);
    }
    if score.relevance_score < 0.3 {
        tracing::info!(
            url = %url,
            relevance = %score.relevance_score,
            "Skipped: low relevance score"
        );
        return Ok(CandidateResult::LowScore);
    }

    // Step 4: Compute final embedding with canonical name
    let final_text =
        EmbeddingService::recipe_summary(&score.canonical_name, &tags, &ingredient_names);
    let final_embedding = embedding_svc
        .embed(&final_text)
        .ok_or_else(|| anyhow::anyhow!("Failed to compute final embedding"))?;

    // Step 5: Insert discovered recipe
    tracing::info!(
        url = %url,
        canonical_name = %score.canonical_name,
        relevance = %score.relevance_score,
        "Inserting discovered recipe"
    );

    // B1: convert ParsedSection to SectionInput and pass through (preserves multi-section structure)
    let sections: Vec<crate::models::SectionInput> = parsed
        .sections
        .iter()
        .enumerate()
        .map(|(idx, s)| crate::models::SectionInput {
            id: None,
            label: s.label.clone(),
            description: s.description.clone(),
            prep_time_min: s.prep_time_min,
            cook_time_min: s.cook_time_min,
            cook_method: s
                .cook_method
                .as_deref()
                .and_then(crate::models::CookMethod::parse),
            sort_order: idx as i32,
            ingredients: s.ingredients.clone(),
            steps: s.steps.clone(),
        })
        .collect();

    let recipe = db::recipes::create_discovered(
        &state.pool,
        owner_id,
        &parsed.title,
        parsed.description.as_deref(),
        url,
        &score.canonical_name,
        score.relevance_score,
        &final_embedding,
        parsed.servings,
        &tags,
        &sections,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to insert discovered recipe: {e}"))?;

    Ok(CandidateResult::Discovered(Box::new(recipe)))
}
