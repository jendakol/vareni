use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::AppState;
use crate::auth::AuthUser;
use crate::db;
use crate::error::AppResult;
use crate::models::{DietaryRestrictionRequest, FoodPreferenceRequest};

pub async fn add_restriction(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<DietaryRestrictionRequest>,
) -> AppResult<StatusCode> {
    db::users::add_dietary_restriction(&state.pool, auth.user_id, &body.restriction).await?;
    Ok(StatusCode::CREATED)
}

pub async fn remove_restriction(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<DietaryRestrictionRequest>,
) -> AppResult<StatusCode> {
    db::users::remove_dietary_restriction(&state.pool, auth.user_id, &body.restriction).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn add_preference(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<FoodPreferenceRequest>,
) -> AppResult<StatusCode> {
    db::users::add_food_preference(&state.pool, auth.user_id, &body.preference).await?;
    Ok(StatusCode::CREATED)
}

pub async fn remove_preference(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<FoodPreferenceRequest>,
) -> AppResult<StatusCode> {
    db::users::remove_food_preference(&state.pool, auth.user_id, &body.preference).await?;
    Ok(StatusCode::NO_CONTENT)
}
