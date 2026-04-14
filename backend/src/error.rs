use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found")]
    NotFound,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound => {
                tracing::debug!("Not found");
                (StatusCode::NOT_FOUND, self.to_string())
            }
            AppError::Unauthorized => {
                tracing::warn!("Unauthorized request");
                (StatusCode::UNAUTHORIZED, self.to_string())
            }
            AppError::BadRequest(msg) => {
                tracing::warn!("Bad request: {msg}");
                (StatusCode::BAD_REQUEST, msg.clone())
            }
            AppError::Conflict(msg) => {
                tracing::warn!("Conflict: {msg}");
                (StatusCode::CONFLICT, msg.clone())
            }
            AppError::ServiceUnavailable(msg) => {
                tracing::warn!("Service unavailable: {msg}");
                (StatusCode::SERVICE_UNAVAILABLE, msg.clone())
            }
            AppError::Internal(err) => {
                tracing::error!("Internal error: {err:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }
            AppError::Sqlx(err) => {
                tracing::error!("Database error: {err}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
