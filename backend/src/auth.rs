use axum::extract::{FromRef, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub exp: i64,
    pub iat: i64,
}

pub fn encode_jwt(user_id: Uuid, secret: &str, expiry_hours: i64) -> anyhow::Result<String> {
    let now = OffsetDateTime::now_utc();
    let claims = Claims {
        sub: user_id,
        iat: now.unix_timestamp(),
        exp: (now + time::Duration::hours(expiry_hours)).unix_timestamp(),
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(token)
}

pub fn decode_jwt(token: &str, secret: &str) -> AppResult<Claims> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::Unauthorized)?;
    Ok(data.claims)
}

/// Extractor that validates JWT and provides the authenticated user ID.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
}

impl<S> FromRequestParts<S> for AuthUser
where
    AppState: axum::extract::FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let token = extract_bearer_token(&parts.headers)?;
        let claims = decode_jwt(&token, &app_state.config.jwt_secret)?;
        Ok(AuthUser {
            user_id: claims.sub,
        })
    }
}

fn extract_bearer_token(headers: &HeaderMap) -> AppResult<String> {
    let header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let token = header
        .strip_prefix("Bearer ")
        .ok_or(AppError::Unauthorized)?;

    Ok(token.to_string())
}
