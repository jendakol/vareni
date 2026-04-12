use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::AppState;
use crate::auth::AuthUser;
use crate::db;
use crate::error::AppResult;
use crate::models::PushSubscriptionRequest;

pub async fn subscribe(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<PushSubscriptionRequest>,
) -> AppResult<StatusCode> {
    db::push::subscribe(&state.pool, auth.user_id, &body.subscription).await?;
    Ok(StatusCode::CREATED)
}

pub async fn unsubscribe(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<PushSubscriptionRequest>,
) -> AppResult<StatusCode> {
    db::push::unsubscribe(&state.pool, auth.user_id, &body.subscription).await?;
    Ok(StatusCode::OK)
}
