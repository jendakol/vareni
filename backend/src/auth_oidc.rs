//! OIDC (Keycloak) login: Authorization Code flow with PKCE, server-side sessions.
//!
//! - `GET /api/auth/login` redirects to the IdP
//! - `GET /api/auth/callback` verifies the ID token, checks the role, maps `sub` to a
//!   pre-seeded user (no JIT provisioning) and creates a session
//! - `POST /api/auth/logout` (form with `csrf_token`) ends the session and the IdP SSO session
//!
//! Every login/callback failure redirects to `/login?error=<code>`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Form;
use axum::extract::rejection::{FormRejection, QueryRejection};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use openidconnect::core::{
    CoreAuthDisplay, CoreAuthPrompt, CoreErrorResponseType, CoreGenderClaim, CoreJsonWebKey,
    CoreJsonWebKeySet, CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm,
    CoreResponseType, CoreRevocableToken, CoreRevocationErrorResponse,
    CoreTokenIntrospectionResponse, CoreTokenType,
};
use openidconnect::url::Url;
use openidconnect::{
    AdditionalClaims, AuthenticationFlow, AuthorizationCode, ClaimsVerificationError, ClientId,
    ClientSecret, CsrfToken, EmptyExtraTokenFields, EndpointMaybeSet, EndpointNotSet, EndpointSet,
    IdToken, IdTokenFields, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    ProviderMetadataWithLogout, RedirectUrl, RequestTokenError, Scope, StandardErrorResponse,
    StandardTokenResponse, TokenResponse,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as AsyncMutex;

use crate::AppState;
use crate::auth::{
    constant_time_eq, extract_secure_cookie, session_cookie_name, session_set_cookie,
};
use crate::config::{Config, OidcConfig};
use crate::db;

/// Holds state/PKCE verifier/nonce/return URL between login and callback.
pub const STATE_COOKIE: &str = "vareni_oidc_state";
const STATE_TTL: Duration = Duration::from_secs(600);
const MAX_STATE_LEN: usize = 128;
const MAX_USED_STATES: usize = 10_000;
const DISCOVERY_RETRY_AFTER: Duration = Duration::from_secs(5);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// Error codes passed to the SPA as `/login?error=<code>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginError {
    IdpUnavailable,
    StateExpired,
    ForbiddenUnknownUser,
    ForbiddenRole,
    InvalidToken,
    LoginFailed,
}

impl LoginError {
    pub fn code(self) -> &'static str {
        match self {
            Self::IdpUnavailable => "idp_unavailable",
            Self::StateExpired => "state_expired",
            Self::ForbiddenUnknownUser => "forbidden_unknown_user",
            Self::ForbiddenRole => "forbidden_role",
            Self::InvalidToken => "invalid_token",
            Self::LoginFailed => "login_failed",
        }
    }
}

/// All non-standard ID token claims (used for the role claim).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraClaims {
    #[serde(flatten)]
    pub other: HashMap<String, serde_json::Value>,
}

impl AdditionalClaims for ExtraClaims {}

type OidcIdToken = IdToken<
    ExtraClaims,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJwsSigningAlgorithm,
>;

type OidcTokenResponse = StandardTokenResponse<
    IdTokenFields<
        ExtraClaims,
        EmptyExtraTokenFields,
        CoreGenderClaim,
        CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm,
    >,
    CoreTokenType,
>;

type OidcClient = openidconnect::Client<
    ExtraClaims,
    CoreAuthDisplay,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJsonWebKey,
    CoreAuthPrompt,
    StandardErrorResponse<CoreErrorResponseType>,
    OidcTokenResponse,
    CoreTokenIntrospectionResponse,
    CoreRevocableToken,
    CoreRevocationErrorResponse,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

struct Provider {
    metadata: ProviderMetadataWithLogout,
    client: OidcClient,
}

#[derive(Default)]
struct ProviderSlot {
    provider: Option<Arc<Provider>>,
    failed_at: Option<Instant>,
}

/// Runtime OIDC state: lazily discovered provider (app boots without the IdP) and
/// consumed login states (replay protection).
pub struct Oidc {
    http: openidconnect::reqwest::Client,
    slot: AsyncMutex<ProviderSlot>,
    used_states: Mutex<HashMap<String, Instant>>,
}

impl Oidc {
    pub fn new() -> anyhow::Result<Self> {
        let http = openidconnect::reqwest::ClientBuilder::new()
            .redirect(openidconnect::reqwest::redirect::Policy::none())
            .timeout(HTTP_TIMEOUT)
            .build()?;
        Ok(Self {
            http,
            slot: AsyncMutex::new(ProviderSlot::default()),
            used_states: Mutex::new(HashMap::new()),
        })
    }

    /// Cached provider, or single-flight discovery (failures retried after a short backoff).
    async fn provider(&self, cfg: &OidcConfig) -> Result<Arc<Provider>, LoginError> {
        let mut slot = self.slot.lock().await;
        if let Some(provider) = &slot.provider {
            return Ok(provider.clone());
        }
        if slot
            .failed_at
            .is_some_and(|t| t.elapsed() < DISCOVERY_RETRY_AFTER)
        {
            return Err(LoginError::IdpUnavailable);
        }
        match self.discover(cfg).await {
            Ok(provider) => {
                let provider = Arc::new(provider);
                slot.provider = Some(provider.clone());
                slot.failed_at = None;
                Ok(provider)
            }
            Err(e) => {
                tracing::error!("OIDC discovery failed: {e:#}");
                slot.failed_at = Some(Instant::now());
                Err(LoginError::IdpUnavailable)
            }
        }
    }

    async fn discover(&self, cfg: &OidcConfig) -> anyhow::Result<Provider> {
        let issuer = IssuerUrl::new(cfg.issuer_url.clone())?;
        let metadata = ProviderMetadataWithLogout::discover_async(issuer, &self.http).await?;
        Self::build_provider(cfg, metadata)
    }

    fn build_provider(
        cfg: &OidcConfig,
        metadata: ProviderMetadataWithLogout,
    ) -> anyhow::Result<Provider> {
        let client = OidcClient::from_provider_metadata(
            metadata.clone(),
            ClientId::new(cfg.client_id.clone()),
            Some(ClientSecret::new(cfg.client_secret.clone())),
        )
        .set_redirect_uri(RedirectUrl::new(cfg.redirect_uri.clone())?);
        Ok(Provider { metadata, client })
    }

    /// Re-fetch JWKS (key rotation) and replace the cached provider.
    async fn refresh_jwks(
        &self,
        cfg: &OidcConfig,
        stale: &Provider,
    ) -> anyhow::Result<Arc<Provider>> {
        let jwks = CoreJsonWebKeySet::fetch_async(stale.metadata.jwks_uri(), &self.http).await?;
        let metadata = stale.metadata.clone().set_jwks(jwks);
        let provider = Arc::new(Self::build_provider(cfg, metadata)?);
        let mut slot = self.slot.lock().await;
        // Another request may have refreshed already; keep its provider.
        match &slot.provider {
            Some(current) if !std::ptr::eq(Arc::as_ptr(current), stale) => Ok(current.clone()),
            _ => {
                slot.provider = Some(provider.clone());
                Ok(provider)
            }
        }
    }

    /// Marks a login state as used; `false` if it was already consumed (replay).
    fn consume_state(&self, state: &str) -> bool {
        if state.len() > MAX_STATE_LEN {
            return false;
        }
        let mut used = self.used_states.lock().unwrap_or_else(|e| e.into_inner());
        used.retain(|_, at| at.elapsed() < STATE_TTL);
        if used.len() >= MAX_USED_STATES {
            // Evict the oldest so forged callbacks can't block real logins.
            if let Some(oldest) = used
                .iter()
                .min_by_key(|(_, at)| **at)
                .map(|(k, _)| k.clone())
            {
                used.remove(&oldest);
            }
        }
        used.insert(state.to_owned(), Instant::now()).is_none()
    }
}

struct VerifiedClaims {
    sub: String,
    has_role: bool,
}

fn verify_id_token(
    provider: &Provider,
    cfg: &OidcConfig,
    id_token: &OidcIdToken,
    nonce: &Nonce,
) -> Result<VerifiedClaims, ClaimsVerificationError> {
    // Asymmetric only: a confidential-client verifier would otherwise accept HS256 keyed
    // with the client secret.
    let verifier = provider
        .client
        .id_token_verifier()
        .set_allowed_algs(vec![CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256]);
    let claims = id_token.claims(&verifier, nonce)?;
    Ok(VerifiedClaims {
        sub: claims.subject().to_string(),
        has_role: has_role(
            &claims.additional_claims().other,
            &cfg.role_claim,
            &cfg.required_role,
        ),
    })
}

/// Looks up `claim_path` (dot separated) and checks for an exact role match in an
/// array or a single string.
fn has_role(claims: &HashMap<String, serde_json::Value>, claim_path: &str, role: &str) -> bool {
    let mut parts = claim_path.split('.');
    let mut value = parts.next().and_then(|first| claims.get(first));
    for part in parts {
        value = value.and_then(|v| v.get(part));
    }
    match value {
        Some(serde_json::Value::Array(items)) => items.iter().any(|v| v.as_str() == Some(role)),
        Some(serde_json::Value::String(s)) => s == role,
        _ => false,
    }
}

/// Only same-origin absolute paths; anything else becomes `/`.
pub fn sanitize_return_url(url: &str) -> String {
    let cleaned: String = url.chars().filter(|c| !c.is_control()).collect();
    let trimmed = cleaned.trim();
    let bytes = trimmed.as_bytes();
    let lower = trimmed.to_ascii_lowercase();
    let protocol_relative = matches!(bytes.get(1), Some(b'/') | Some(b'\\'))
        || lower.starts_with("/%5c")
        || lower.starts_with("/%2f");
    if bytes.first() == Some(&b'/') && !protocol_relative {
        trimmed.to_owned()
    } else {
        "/".to_owned()
    }
}

fn state_set_cookie(config: &Config, value: &str, max_age_secs: u64) -> String {
    let secure = if config.cookie_secure { "; Secure" } else { "" };
    format!(
        "{STATE_COOKIE}={value}; Path=/api/auth; HttpOnly; SameSite=Lax{secure}; Max-Age={max_age_secs}"
    )
}

/// 302 with optional `Set-Cookie` headers.
fn redirect_with_cookies(location: &str, cookies: &[String]) -> Response {
    let mut response = (StatusCode::FOUND, [(header::LOCATION, location)]).into_response();
    for cookie in cookies {
        if let Ok(v) = HeaderValue::from_str(cookie) {
            response.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    response
}

fn login_error(config: &Config, err: LoginError) -> Response {
    redirect_with_cookies(
        &format!("/login?error={}", err.code()),
        &[state_set_cookie(config, "", 0)],
    )
}

#[derive(Debug, Deserialize)]
pub struct LoginParams {
    return_url: Option<String>,
}

/// `GET /api/auth/login`
pub async fn login(
    State(state): State<AppState>,
    params: Result<Query<LoginParams>, QueryRejection>,
) -> Response {
    let config = &state.config;
    let Some(cfg) = config.oidc.as_ref() else {
        tracing::error!("Login attempted but OIDC is not configured");
        return login_error(config, LoginError::IdpUnavailable);
    };
    let provider = match state.oidc.provider(cfg).await {
        Ok(p) => p,
        Err(e) => return login_error(config, e),
    };

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, csrf_state, nonce) = provider
        .client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("profile".into()))
        .add_scope(Scope::new("email".into()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    let return_url = params
        .ok()
        .and_then(|Query(p)| p.return_url)
        .map(|u| sanitize_return_url(&u))
        .unwrap_or_else(|| "/".into());
    // Random parts are base64url (no ':'); the return URL is percent-encoded.
    let value = format!(
        "{}:{}:{}:{}",
        csrf_state.secret(),
        pkce_verifier.secret(),
        nonce.secret(),
        urlencoding::encode(&return_url)
    );

    redirect_with_cookies(
        auth_url.as_str(),
        &[state_set_cookie(config, &value, STATE_TTL.as_secs())],
    )
}

#[derive(Deserialize)]
pub struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

struct LoginState {
    csrf: String,
    pkce_verifier: String,
    nonce: String,
    return_url: String,
}

fn parse_state_cookie(value: &str) -> Option<LoginState> {
    let mut parts = value.splitn(4, ':');
    let csrf = parts.next()?.to_owned();
    let pkce_verifier = parts.next()?.to_owned();
    let nonce = parts.next()?.to_owned();
    let return_url = urlencoding::decode(parts.next()?).ok()?;
    if csrf.is_empty() || pkce_verifier.is_empty() || nonce.is_empty() {
        return None;
    }
    Some(LoginState {
        csrf,
        pkce_verifier,
        nonce,
        return_url: sanitize_return_url(&return_url),
    })
}

/// `GET /api/auth/callback`
pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    params: Result<Query<CallbackParams>, QueryRejection>,
) -> Response {
    let config = &state.config;
    match complete_login(&state, &headers, params).await {
        Ok((session, return_url)) => {
            let max_age = config.session_ttl_hours * 3600;
            redirect_with_cookies(
                &return_url,
                &[
                    session_set_cookie(config, &session.raw_id, max_age),
                    state_set_cookie(config, "", 0),
                ],
            )
        }
        Err(e) => login_error(config, e),
    }
}

async fn complete_login(
    state: &AppState,
    headers: &HeaderMap,
    params: Result<Query<CallbackParams>, QueryRejection>,
) -> Result<(db::sessions::NewSession, String), LoginError> {
    let Ok(Query(params)) = params else {
        return Err(LoginError::LoginFailed);
    };
    if let Some(error) = &params.error {
        let error: String = error.chars().take(64).collect();
        tracing::warn!(error = %error, "IdP returned an error to the callback");
        return Err(LoginError::LoginFailed);
    }
    let cfg = state
        .config
        .oidc
        .as_ref()
        .ok_or(LoginError::IdpUnavailable)?;

    let login_state = extract_secure_cookie(headers, STATE_COOKIE)
        .ok()
        .flatten()
        .and_then(|v| parse_state_cookie(&v))
        .ok_or(LoginError::StateExpired)?;
    let returned_state = params.state.as_deref().unwrap_or_default();
    if !constant_time_eq(returned_state, &login_state.csrf) {
        tracing::warn!("OIDC callback state mismatch");
        return Err(LoginError::StateExpired);
    }
    let code = params.code.ok_or(LoginError::LoginFailed)?;
    if !state.oidc.consume_state(&login_state.csrf) {
        tracing::warn!("OIDC callback state replayed");
        return Err(LoginError::StateExpired);
    }

    let provider = state.oidc.provider(cfg).await?;
    let token_response = provider
        .client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(|e| {
            tracing::error!("IdP has no token endpoint: {e}");
            LoginError::IdpUnavailable
        })?
        .set_pkce_verifier(PkceCodeVerifier::new(login_state.pkce_verifier))
        .request_async(&state.oidc.http)
        .await
        .map_err(|e| match e {
            RequestTokenError::ServerResponse(r) => {
                tracing::warn!(error = %r.error(), "Token exchange rejected");
                LoginError::LoginFailed
            }
            other => {
                tracing::error!("Token exchange failed: {other}");
                LoginError::IdpUnavailable
            }
        })?;

    let id_token = token_response.id_token().ok_or_else(|| {
        tracing::warn!("Token response has no ID token");
        LoginError::InvalidToken
    })?;
    let nonce = Nonce::new(login_state.nonce);
    let claims = match verify_id_token(&provider, cfg, id_token, &nonce) {
        Err(ClaimsVerificationError::SignatureVerification(e)) => {
            tracing::info!("ID token signature check failed ({e}), refreshing JWKS");
            let provider = state.oidc.refresh_jwks(cfg, &provider).await.map_err(|e| {
                tracing::error!("JWKS refresh failed: {e:#}");
                LoginError::IdpUnavailable
            })?;
            verify_id_token(&provider, cfg, id_token, &nonce)
        }
        other => other,
    }
    .map_err(|e| {
        tracing::warn!("ID token verification failed: {e}");
        LoginError::InvalidToken
    })?;

    if !claims.has_role {
        tracing::warn!(sub = %claims.sub, "Login denied: required role missing");
        return Err(LoginError::ForbiddenRole);
    }
    let user = db::users::find_by_external_id(&state.pool, &claims.sub)
        .await
        .map_err(|e| {
            tracing::error!("User lookup failed: {e}");
            LoginError::LoginFailed
        })?
        .ok_or_else(|| {
            tracing::warn!(sub = %claims.sub, "Login denied: no user linked to this subject");
            LoginError::ForbiddenUnknownUser
        })?;

    // Never reuse a pre-existing session id (fixation); drop the old one if present.
    if let Ok(Some(old)) = extract_secure_cookie(headers, session_cookie_name(&state.config)) {
        let _ = db::sessions::delete(&state.pool, &old).await;
    }
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let session = db::sessions::create(
        &state.pool,
        user.id,
        Some(&id_token.to_string()),
        user_agent,
        state.config.session_ttl_hours,
    )
    .await
    .map_err(|e| {
        tracing::error!("Session creation failed: {e:#}");
        LoginError::LoginFailed
    })?;

    tracing::info!(user_id = %user.id, "OIDC login successful");
    Ok((session, login_state.return_url))
}

#[derive(Deserialize)]
pub struct LogoutForm {
    csrf_token: String,
}

/// `POST /api/auth/logout` — always clears the cookie and redirects, never returns 401/403.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    form: Result<Form<LogoutForm>, FormRejection>,
) -> Response {
    let config = &state.config;
    let mut location = "/login".to_owned();

    if let Ok(Some(raw_id)) = extract_secure_cookie(&headers, session_cookie_name(config)) {
        match db::sessions::find_valid(&state.pool, &raw_id).await {
            Ok(Some(session)) => {
                let csrf_ok = form.as_ref().is_ok_and(|Form(f)| {
                    !f.csrf_token.is_empty() && constant_time_eq(&f.csrf_token, &session.csrf_token)
                });
                if csrf_ok {
                    if let Err(e) = db::sessions::delete(&state.pool, &raw_id).await {
                        tracing::error!("Failed to delete session on logout: {e}");
                    }
                    if let Some(url) = end_session_url(&state, session.id_token.as_deref()).await {
                        location = url;
                    }
                } else {
                    tracing::warn!("Logout with invalid CSRF token, session kept");
                }
            }
            Ok(None) => {}
            Err(e) => tracing::error!("Session lookup on logout failed: {e}"),
        }
    }

    let mut response = Redirect::to(&location).into_response();
    if let Ok(v) = HeaderValue::from_str(&session_set_cookie(config, "", 0)) {
        response.headers_mut().append(header::SET_COOKIE, v);
    }
    response
}

/// RP-initiated logout URL, if the IdP supports it and we have an ID token hint.
async fn end_session_url(state: &AppState, id_token: Option<&str>) -> Option<String> {
    let cfg = state.config.oidc.as_ref()?;
    let id_token = id_token?;
    let provider = state.oidc.provider(cfg).await.ok()?;
    let endpoint = provider
        .metadata
        .additional_metadata()
        .end_session_endpoint
        .as_ref()?;
    let mut url = Url::parse(endpoint.url().as_str()).ok()?;
    url.query_pairs_mut()
        .append_pair("id_token_hint", id_token)
        .append_pair("post_logout_redirect_uri", &cfg.post_logout_redirect_uri)
        .append_pair("client_id", &cfg.client_id);
    Some(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sanitize_allows_relative_paths() {
        assert_eq!(sanitize_return_url("/plan"), "/plan");
        assert_eq!(sanitize_return_url("/recipes/1?x=1#y"), "/recipes/1?x=1#y");
        assert_eq!(sanitize_return_url("/"), "/");
        assert_eq!(sanitize_return_url("  /plan  "), "/plan");
    }

    #[test]
    fn sanitize_blocks_absolute_and_protocol_relative() {
        for bad in [
            "https://evil.com",
            "http://evil.com/x",
            "//evil.com",
            "  //evil.com",
            r"/\evil.com",
            r"/\/evil.com",
            "/%5Cevil.com",
            "/%5cevil.com",
            "/%2Fevil.com",
            "/%2f%2fevil.com",
            "javascript:alert(1)",
            "evil.com",
            "",
        ] {
            assert_eq!(sanitize_return_url(bad), "/", "{bad}");
        }
    }

    #[test]
    fn sanitize_strips_control_characters() {
        assert_eq!(sanitize_return_url("/a\tb\r\nc"), "/abc");
        assert_eq!(sanitize_return_url("/\n/evil.com"), "/");
    }

    #[test]
    fn state_cookie_roundtrip() {
        let value = format!("s:v:n:{}", urlencoding::encode("/recipes?a=1;b=2"));
        let parsed = parse_state_cookie(&value).unwrap();
        assert_eq!(parsed.csrf, "s");
        assert_eq!(parsed.pkce_verifier, "v");
        assert_eq!(parsed.nonce, "n");
        assert_eq!(parsed.return_url, "/recipes?a=1;b=2");
    }

    #[test]
    fn state_cookie_malformed() {
        assert!(parse_state_cookie("s:v").is_none());
        assert!(parse_state_cookie("s:v:n").is_none());
        assert!(parse_state_cookie(":v:n:%2F").is_none());
    }

    #[test]
    fn state_cookie_return_url_is_sanitized() {
        let value = format!("s:v:n:{}", urlencoding::encode("//evil.com"));
        assert_eq!(parse_state_cookie(&value).unwrap().return_url, "/");
    }

    fn claims(v: serde_json::Value) -> HashMap<String, serde_json::Value> {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn role_in_array() {
        let c = claims(json!({ "roles": ["a", "vareni"] }));
        assert!(has_role(&c, "roles", "vareni"));
        assert!(!has_role(&c, "roles", "admin"));
    }

    #[test]
    fn role_as_string() {
        let c = claims(json!({ "roles": "vareni" }));
        assert!(has_role(&c, "roles", "vareni"));
    }

    #[test]
    fn role_dotted_path() {
        let c = claims(json!({ "realm_access": { "roles": ["vareni"] } }));
        assert!(has_role(&c, "realm_access.roles", "vareni"));
        assert!(!has_role(&c, "roles", "vareni"));
    }

    #[test]
    fn role_requires_exact_match() {
        let c = claims(json!({ "roles": ["vareni-admin", "Vareni", ["vareni"]] }));
        assert!(!has_role(&c, "roles", "vareni"));
        let c = claims(json!({ "roles": "vareni,admin" }));
        assert!(!has_role(&c, "roles", "vareni"));
        assert!(!has_role(&claims(json!({})), "roles", "vareni"));
    }

    #[test]
    fn replayed_state_is_rejected() {
        let oidc = Oidc::new().unwrap();
        assert!(oidc.consume_state("abc"));
        assert!(!oidc.consume_state("abc"));
        assert!(oidc.consume_state("def"));
        assert!(!oidc.consume_state(&"x".repeat(MAX_STATE_LEN + 1)));
    }
}
