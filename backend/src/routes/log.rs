use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::AppState;
use crate::auth::ApiToken;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::metrics::MEAL_LOG_ENTRIES_TOTAL;
use crate::models::{CreateMealPlanRequest, MealPlanEntry};

#[derive(Debug, Deserialize)]
pub struct CreateLogRequest {
    /// YYYY-MM-DD; defaults to today (UTC)
    pub date: Option<String>,
    /// "lunch" | "dinner"
    pub meal_type: String,
    pub free_text: Option<String>,
    pub recipe_id: Option<Uuid>,
    /// Username to attribute the entry to; omit to create without a user
    pub user_name: Option<String>,
}

pub async fn create_entry(
    State(state): State<AppState>,
    _token: ApiToken,
    Json(body): Json<CreateLogRequest>,
) -> AppResult<(StatusCode, Json<MealPlanEntry>)> {
    let date = body.date.unwrap_or_else(|| {
        let now = OffsetDateTime::now_utc();
        format!(
            "{:04}-{:02}-{:02}",
            now.year(),
            now.month() as u8,
            now.day()
        )
    });

    let user_id = if let Some(name) = &body.user_name {
        db::users::find_by_name(&state.pool, name)
            .await?
            .map(|u| u.id)
            .ok_or_else(|| AppError::NotFound)?
    } else {
        return Err(AppError::BadRequest("user_name is required".into()));
    };

    let req = CreateMealPlanRequest {
        date,
        meal_type: body.meal_type,
        recipe_id: body.recipe_id,
        free_text: body.free_text,
        servings: None,
        status: Some("confirmed".into()),
        entry_type: Some("logged".into()),
        note: None,
        for_user_id: None,
    };

    let entry = db::meal_plan::create(&state.pool, user_id, &req).await?;
    metrics::counter!(
        MEAL_LOG_ENTRIES_TOTAL,
        "meal_type" => req.meal_type.clone(),
        "source" => "ha_api",
    )
    .increment(1);
    Ok((StatusCode::CREATED, Json(entry)))
}
