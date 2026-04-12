use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use uuid::Uuid;

use crate::AppState;
use crate::ai;
use crate::ai::client::AnthropicClient;
use crate::auth::AuthUser;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::{
    CreateMealPlanRequest, MealPlanEntry, MealPlanHistoryQuery, MealPlanQuery,
    UpdateMealPlanRequest,
};

#[derive(Debug, serde::Deserialize)]
pub struct SuggestRequest {
    pub prompt: String,
}

pub async fn suggest(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<SuggestRequest>,
) -> AppResult<Json<Vec<ai::plan::SuggestedEntry>>> {
    let history = db::meal_plan::history(&state.pool, 90).await?;
    let restrictions = db::users::get_dietary_restrictions(&state.pool, auth.user_id).await?;
    let (recipes, _) = db::recipes::list(&state.pool, None, None, 1, 1000).await?;

    let history_json = serde_json::to_string(&history).unwrap_or_default();
    let restrictions_json = serde_json::to_string(&restrictions).unwrap_or_default();
    let recipes_json = serde_json::to_string(&recipes).unwrap_or_default();

    let ai_client = AnthropicClient::new(&state.config.anthropic_api_key);
    let suggestions = ai::plan::suggest(
        &ai_client,
        &history_json,
        &restrictions_json,
        &recipes_json,
        &body.prompt,
    )
    .await
    .map_err(AppError::Internal)?;

    Ok(Json(suggestions))
}

pub async fn list(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(query): Query<MealPlanQuery>,
) -> AppResult<Json<Vec<MealPlanEntry>>> {
    let entries = db::meal_plan::list_by_range(&state.pool, &query.from, &query.to).await?;
    Ok(Json(entries))
}

pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateMealPlanRequest>,
) -> AppResult<(StatusCode, Json<MealPlanEntry>)> {
    let entry = db::meal_plan::create(&state.pool, auth.user_id, &body).await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

pub async fn update(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateMealPlanRequest>,
) -> AppResult<Json<MealPlanEntry>> {
    let entry = db::meal_plan::update(&state.pool, id, &body)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(entry))
}

pub async fn delete(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let deleted = db::meal_plan::delete(&state.pool, id).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

pub async fn history(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(query): Query<MealPlanHistoryQuery>,
) -> AppResult<Json<Vec<MealPlanEntry>>> {
    let days = query.days.unwrap_or(90);
    let entries = db::meal_plan::history(&state.pool, days).await?;
    Ok(Json(entries))
}
