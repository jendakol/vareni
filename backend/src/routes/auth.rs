use axum::Json;
use axum::extract::State;

use crate::AppState;
use crate::auth::AuthUser;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::{User, UserProfile};

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
        csrf_token: auth.csrf_token,
    }))
}

pub async fn list_users(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> AppResult<Json<Vec<User>>> {
    let users = db::users::list_all(&state.pool).await?;
    Ok(Json(users))
}
