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
    let mut image_data = None;
    let mut image_media_type = None;
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
                image_media_type = Some(content_type);
                image_data = Some(data);
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
                .ok_or_else(|| AppError::BadRequest("text is required for manual source".into()))?;
            ai::ingest::parse_text(&ai_client, &text)
                .await
                .map_err(AppError::Internal)?
        }
        "photo" => {
            let data = image_data
                .ok_or_else(|| AppError::BadRequest("image is required for photo source".into()))?;
            let media_type = image_media_type.unwrap_or("image/jpeg".into());
            ai::ingest::parse_image(&ai_client, &data, &media_type)
                .await
                .map_err(AppError::Internal)?
        }
        "url" => {
            let url =
                url.ok_or_else(|| AppError::BadRequest("url is required for url source".into()))?;
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
