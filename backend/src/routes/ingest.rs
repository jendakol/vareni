use axum::Json;
use axum::extract::{Multipart, State};

use crate::AppState;
use crate::ai;
use crate::ai::client::AnthropicClient;
use crate::ai::ingest::ParsedRecipe;
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};

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

    let parsed = match source_type.as_str() {
        "manual" => {
            let text = text
                .filter(|t| !t.trim().is_empty())
                .ok_or_else(|| AppError::BadRequest("Zadejte text receptu".into()))?;
            ai::ingest::parse_text(&ai_client, &text)
                .await
                .map_err(AppError::Internal)?
        }
        "photo" => {
            if images.is_empty() {
                return Err(AppError::BadRequest("Nahrajte fotku receptu".into()));
            }
            let image_refs: Vec<(&[u8], &str)> = images
                .iter()
                .map(|(data, mt)| (data.as_ref(), mt.as_str()))
                .collect();
            ai::ingest::parse_images(&ai_client, &image_refs)
                .await
                .map_err(AppError::Internal)?
        }
        "url" => {
            let url = url
                .filter(|u| !u.trim().is_empty())
                .ok_or_else(|| AppError::BadRequest("Zadejte URL receptu".into()))?;
            ai::ingest::parse_url(&ai_client, &reqwest::Client::new(), &url)
                .await
                .map_err(AppError::Internal)?
        }
        other => {
            return Err(AppError::BadRequest(format!(
                "unknown source_type: {other}"
            )));
        }
    };

    Ok(Json(parsed))
}
