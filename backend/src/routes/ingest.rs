use axum::Json;
use axum::extract::{Multipart, State};

use crate::AppState;
use crate::ai;
use crate::ai::client::AnthropicClient;
use crate::ai::ingest::ParsedRecipe;
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::metrics::RECIPE_INGESTS_TOTAL;

pub async fn ingest(
    State(state): State<AppState>,
    _auth: AuthUser,
    mut multipart: Multipart,
) -> AppResult<Json<ParsedRecipe>> {
    let mut source_type = None;
    let mut text = None;
    let mut images: Vec<(bytes::Bytes, String)> = Vec::new();
    let mut url = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "source_type" => {
                source_type = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::BadRequest(e.to_string()))?,
                )
            }
            "text" => {
                text = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::BadRequest(e.to_string()))?,
                )
            }
            "url" => {
                url = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::BadRequest(e.to_string()))?,
                )
            }
            "image" => {
                let content_type = field.content_type().unwrap_or("image/jpeg").to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                images.push((data, content_type));
            }
            _ => {}
        }
    }

    let source_type =
        source_type.ok_or_else(|| AppError::BadRequest("source_type is required".into()))?;
    let ai_client = AnthropicClient::new(&state.config.anthropic_api_key);

    let parsed_result: AppResult<ParsedRecipe> = match source_type.as_str() {
        "manual" => {
            let text = text
                .filter(|t| !t.trim().is_empty())
                .ok_or_else(|| AppError::BadRequest("Zadejte text receptu".into()));
            match text {
                Ok(t) => ai::ingest::parse_text(&ai_client, &t)
                    .await
                    .map_err(AppError::Internal),
                Err(e) => Err(e),
            }
        }
        "photo" => {
            if images.is_empty() {
                Err(AppError::BadRequest("Nahrajte fotku receptu".into()))
            } else {
                let image_refs: Vec<(&[u8], &str)> = images
                    .iter()
                    .map(|(data, mt)| (data.as_ref(), mt.as_str()))
                    .collect();
                ai::ingest::parse_images(&ai_client, &image_refs)
                    .await
                    .map_err(AppError::Internal)
            }
        }
        "url" => ingest_from_url(&state, &ai_client, url).await,
        other => Err(AppError::BadRequest(format!(
            "unknown source_type: {other}"
        ))),
    };

    let status_label = if parsed_result.is_ok() { "ok" } else { "error" };
    metrics::counter!(
        RECIPE_INGESTS_TOTAL,
        "source" => source_type.clone(),
        "status" => status_label,
    )
    .increment(1);

    let parsed = parsed_result?;
    Ok(Json(parsed))
}

async fn ingest_from_url(
    state: &AppState,
    ai_client: &AnthropicClient,
    url: Option<String>,
) -> AppResult<ParsedRecipe> {
    let url = url
        .filter(|u| !u.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("Zadejte URL receptu".into()))?;

    let needs_browser = crate::scraper::needs_browser(&url);
    let _browser_permit = if needs_browser {
        Some(
            state
                .browser_semaphore
                .acquire()
                .await
                .map_err(|_| AppError::ServiceUnavailable("Browser unavailable".into()))?,
        )
    } else {
        None
    };
    let _browser_handle;
    let browser = if needs_browser {
        match crate::browser::launch().await {
            Ok((b, handle)) => {
                _browser_handle = Some(handle);
                Some(b)
            }
            Err(e) => {
                return Err(AppError::ServiceUnavailable(format!(
                    "Tato stránka vyžaduje prohlížeč, který se nepodařilo spustit: {e}"
                )));
            }
        }
    } else {
        _browser_handle = None;
        None
    };

    ai::ingest::parse_url(ai_client, &state.http_client, browser.as_ref(), &url)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.starts_with("Nepodařilo") {
                AppError::BadRequest(msg)
            } else {
                AppError::Internal(e)
            }
        })
}
