use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::Response;
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;
use crate::ai;
use crate::ai::client::{AnthropicClient, Message};
use crate::auth::AuthUser;
use crate::db;
use crate::error::{AppError, AppResult};

const CHAT_MODEL: &str = "claude-sonnet-4-6";

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub session_id: Option<String>,
}

pub async fn chat(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(recipe_id): Path<Uuid>,
    Json(body): Json<ChatRequest>,
) -> AppResult<Response> {
    let recipe = db::recipes::get_by_id(&state.pool, recipe_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let recipe_json = serde_json::to_string(&recipe).unwrap_or_default();
    let system = ai::chat::system_prompt(&recipe_json);
    let tools = vec![ai::chat::update_recipe_tool()];

    let messages = vec![Message {
        role: "user".into(),
        content: serde_json::json!(body.message),
    }];

    let ai_client = AnthropicClient::new(&state.config.anthropic_api_key);
    let byte_stream = ai_client
        .stream_raw(CHAT_MODEL, &system, messages, Some(tools), 4096)
        .await
        .map_err(AppError::Internal)?;

    let body = Body::from_stream(byte_stream);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(body)
        .unwrap())
}
