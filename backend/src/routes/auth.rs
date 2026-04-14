use axum::Json;
use axum::extract::State;

use crate::AppState;
use crate::auth::{AuthUser, encode_jwt};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::{LoginRequest, LoginResponse, User, UserProfile};

pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> AppResult<Json<LoginResponse>> {
    let user = db::users::find_by_name(&state.pool, &body.name)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let valid = bcrypt::verify(&body.password, &user.password_hash)
        .map_err(|e| AppError::Internal(e.into()))?;
    if !valid {
        return Err(AppError::Unauthorized);
    }

    let token = encode_jwt(
        user.id,
        &state.config.jwt_secret,
        state.config.jwt_expiry_hours,
    )
    .map_err(AppError::Internal)?;

    Ok(Json(LoginResponse { token, user }))
}

pub async fn me(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<UserProfile>> {
    let user = db::users::find_by_id(&state.pool, auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let restrictions = db::users::get_dietary_restrictions(&state.pool, auth.user_id).await?;
    let preferences = db::users::get_food_preferences(&state.pool, auth.user_id).await?;

    Ok(Json(UserProfile {
        user,
        dietary_restrictions: restrictions,
        food_preferences: preferences,
    }))
}

pub async fn list_users(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> AppResult<Json<Vec<User>>> {
    let users = db::users::list_all(&state.pool).await?;
    Ok(Json(users))
}
