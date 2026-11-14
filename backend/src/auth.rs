use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, header};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::AppState;
use crate::config::Config;
use crate::db;
use crate::error::{AppError, AppResult};

/// Production session cookie; `__Host-` forces Secure, Path=/ and no Domain.
pub const SESSION_COOKIE: &str = "__Host-vareni_session";
/// Local-dev session cookie (plain HTTP), only with `ENVIRONMENT=local` + `COOKIE_SECURE=false`.
pub const DEV_SESSION_COOKIE: &str = "vareni_session";
pub const CSRF_HEADER: &str = "X-CSRF-Token";

pub fn session_cookie_name(config: &Config) -> &'static str {
    if config.cookie_secure {
        SESSION_COOKIE
    } else {
        DEV_SESSION_COOKIE
    }
}

/// `Set-Cookie` value for the session cookie; empty value + `max_age_secs = 0` clears it.
pub fn session_set_cookie(config: &Config, value: &str, max_age_secs: i64) -> String {
    let secure = if config.cookie_secure { "; Secure" } else { "" };
    format!(
        "{}={value}; Path=/; HttpOnly; SameSite=Lax{secure}; Max-Age={max_age_secs}",
        session_cookie_name(config)
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuplicateCookie;

/// Single non-empty cookie value; a cookie present more than once is an error.
pub fn extract_secure_cookie(
    headers: &HeaderMap,
    name: &str,
) -> Result<Option<String>, DuplicateCookie> {
    let prefix = format!("{name}=");
    let mut found = None;
    for value in headers.get_all(header::COOKIE) {
        let Ok(value) = value.to_str() else { continue };
        for pair in value.split(';').map(str::trim) {
            if let Some(v) = pair.strip_prefix(&prefix) {
                if found.is_some() {
                    return Err(DuplicateCookie);
                }
                found = Some(v.to_string());
            }
        }
    }
    Ok(found.filter(|v| !v.is_empty()))
}

pub fn constant_time_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

fn is_safe_method(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// Session-cookie authenticated user. Unsafe methods also require a valid `X-CSRF-Token`.
#[derive(Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub csrf_token: String,
}

impl std::fmt::Debug for AuthUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthUser")
            .field("user_id", &self.user_id)
            .finish_non_exhaustive()
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        #[cfg(feature = "debug-auth")]
        if let Some(user) = debug_user(&app_state, &parts.headers).await? {
            return Ok(user);
        }

        // Fail closed: without OIDC nobody has a legitimate session.
        if !sessions_enabled(&app_state.config) {
            return Err(AppError::Unauthorized);
        }

        let raw_id = extract_secure_cookie(&parts.headers, session_cookie_name(&app_state.config))
            .map_err(|_| AppError::BadRequest("Duplicate session cookie".into()))?
            .ok_or(AppError::Unauthorized)?;
        let session = db::sessions::find_valid(&app_state.pool, &raw_id)
            .await?
            .ok_or(AppError::Unauthorized)?;

        if !is_safe_method(&parts.method) {
            let provided = parts
                .headers
                .get(CSRF_HEADER)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default();
            if provided.is_empty() || !constant_time_eq(provided, &session.csrf_token) {
                return Err(AppError::Forbidden("Invalid CSRF token".into()));
            }
        }

        Ok(AuthUser {
            user_id: session.user_id,
            csrf_token: session.csrf_token,
        })
    }
}

/// Sessions are honoured with OIDC configured, or locally with `debug-auth` (dev-login).
fn sessions_enabled(config: &Config) -> bool {
    config.oidc.is_some() || (cfg!(feature = "debug-auth") && config.is_local())
}

#[cfg(feature = "debug-auth")]
#[derive(serde::Deserialize)]
pub struct DevLoginParams {
    account: String,
}

/// `GET /api/auth/dev-login?account=<name | 1-based index>`: local login without Keycloak.
#[cfg(feature = "debug-auth")]
pub async fn dev_login(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Query(params): axum::extract::Query<DevLoginParams>,
) -> AppResult<axum::response::Response> {
    use axum::http::{HeaderValue, StatusCode};
    use axum::response::IntoResponse;

    if !state.config.is_local() {
        return Err(AppError::NotFound);
    }
    let users = db::users::list_all(&state.pool).await?;
    let user = match params.account.parse::<usize>() {
        Ok(n) => n.checked_sub(1).and_then(|i| users.into_iter().nth(i)),
        Err(_) => users
            .into_iter()
            .find(|u| u.name.eq_ignore_ascii_case(&params.account)),
    }
    .ok_or(AppError::NotFound)?;

    let session = db::sessions::create(
        &state.pool,
        user.id,
        None,
        Some("dev-login"),
        state.config.session_ttl_hours,
    )
    .await?;
    tracing::warn!(user = %user.name, "Dev login used");

    let max_age = state.config.session_ttl_hours * 3600;
    let mut response = (StatusCode::FOUND, [(header::LOCATION, "/")]).into_response();
    if let Ok(v) =
        HeaderValue::from_str(&session_set_cookie(&state.config, &session.raw_id, max_age))
    {
        response.headers_mut().append(header::SET_COOKIE, v);
    }
    Ok(response)
}

/// `X-Debug-User: <name>` login; compiled only with `debug-auth` and active only when local.
#[cfg(feature = "debug-auth")]
async fn debug_user(state: &AppState, headers: &HeaderMap) -> AppResult<Option<AuthUser>> {
    if !state.config.is_local() {
        return Ok(None);
    }
    let Some(name) = headers.get("X-Debug-User").and_then(|v| v.to_str().ok()) else {
        return Ok(None);
    };
    let user = db::users::find_by_name(&state.pool, name)
        .await?
        .ok_or(AppError::Unauthorized)?;
    tracing::warn!(user = %user.name, "Debug auth used");
    Ok(Some(AuthUser {
        user_id: user.id,
        csrf_token: "debug".into(),
    }))
}

/// Extractor that validates a static API token (for server-to-server calls, e.g. Home Assistant).
pub struct ApiToken;

impl<S> FromRequestParts<S> for ApiToken
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let configured = app_state
            .config
            .log_api_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or(AppError::Unauthorized)?;
        let provided = extract_bearer_token(&parts.headers)?;
        if !constant_time_eq(&provided, configured) {
            return Err(AppError::Unauthorized);
        }
        Ok(ApiToken)
    }
}

fn extract_bearer_token(headers: &HeaderMap) -> AppResult<String> {
    let header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let token = header
        .strip_prefix("Bearer ")
        .ok_or(AppError::Unauthorized)?;

    Ok(token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(cookie: &'static str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_static(cookie));
        h
    }

    #[test]
    fn extracts_single_cookie() {
        let h = headers("a=1; __Host-vareni_session=abc; b=2");
        assert_eq!(
            extract_secure_cookie(&h, SESSION_COOKIE),
            Ok(Some("abc".into()))
        );
    }

    #[test]
    fn missing_or_empty_cookie_is_none() {
        assert_eq!(
            extract_secure_cookie(&headers("a=1"), SESSION_COOKIE),
            Ok(None)
        );
        assert_eq!(
            extract_secure_cookie(&headers("__Host-vareni_session="), SESSION_COOKIE),
            Ok(None)
        );
        assert_eq!(
            extract_secure_cookie(&HeaderMap::new(), SESSION_COOKIE),
            Ok(None)
        );
    }

    #[test]
    fn duplicate_cookie_is_rejected() {
        let h = headers("__Host-vareni_session=a; __Host-vareni_session=b");
        assert_eq!(
            extract_secure_cookie(&h, SESSION_COOKIE),
            Err(DuplicateCookie)
        );
    }

    #[test]
    fn duplicate_across_header_lines_is_rejected() {
        let mut h = headers("__Host-vareni_session=a");
        h.append(
            header::COOKIE,
            HeaderValue::from_static("__Host-vareni_session=b"),
        );
        assert_eq!(
            extract_secure_cookie(&h, SESSION_COOKIE),
            Err(DuplicateCookie)
        );
    }

    #[test]
    fn similar_cookie_names_do_not_match() {
        let h = headers("x__Host-vareni_session=evil; vareni_session=dev");
        assert_eq!(extract_secure_cookie(&h, SESSION_COOKIE), Ok(None));
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
    }
}
