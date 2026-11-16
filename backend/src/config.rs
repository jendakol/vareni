use std::env;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub anthropic_api_key: String,
    /// `local` enables dev-only behaviour (insecure cookies, debug auth).
    pub environment: String,
    pub cookie_secure: bool,
    pub session_ttl_hours: i64,
    /// `None` = OIDC not configured; nobody can log in (fail closed).
    pub oidc: Option<OidcConfig>,
    pub base_url: String,
    pub static_dir: String,
    pub upload_dir: String,
    // Discovery
    pub embedding_model_dir: Option<String>,
    pub discovery_enabled: bool,
    // Log API (Home Assistant)
    pub log_api_token: Option<String>,
    // Metrics
    pub metrics_gauge_refresh_secs: u64,
    /// Never read from the environment: always `false` outside tests.
    pub image_fetch_allow_private: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let environment = env::var("ENVIRONMENT").unwrap_or_else(|_| "production".into());
        let cookie_secure: bool = env::var("COOKIE_SECURE")
            .unwrap_or_else(|_| "true".into())
            .parse()?;
        anyhow::ensure!(
            cookie_secure || environment == "local",
            "COOKIE_SECURE=false is only allowed with ENVIRONMENT=local"
        );
        let base_url = env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:8080".into());

        Ok(Self {
            database_url: env::var("DATABASE_URL")?,
            anthropic_api_key: env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
            environment,
            cookie_secure,
            session_ttl_hours: env::var("SESSION_TTL_HOURS")
                .unwrap_or_else(|_| "720".into())
                .parse()?,
            oidc: OidcConfig::from_env(&base_url)?,
            base_url,
            static_dir: env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into()),
            upload_dir: env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".into()),
            embedding_model_dir: env::var("EMBEDDING_MODEL_DIR").ok(),
            discovery_enabled: env::var("DISCOVERY_ENABLED")
                .unwrap_or_else(|_| "true".into())
                .parse()
                .unwrap_or(true),
            log_api_token: env::var("LOG_API_TOKEN").ok(),
            metrics_gauge_refresh_secs: env::var("METRICS_GAUGE_REFRESH_SECS")
                .unwrap_or_else(|_| "60".into())
                .parse()
                .unwrap_or(60),
            image_fetch_allow_private: false,
        })
    }

    pub fn is_local(&self) -> bool {
        self.environment == "local"
    }
}

#[derive(Clone)]
pub struct OidcConfig {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub post_logout_redirect_uri: String,
    pub required_role: String,
    /// Claim holding the roles; dotted paths (e.g. `realm_access.roles`) are supported.
    pub role_claim: String,
}

impl std::fmt::Debug for OidcConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcConfig")
            .field("issuer_url", &self.issuer_url)
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("redirect_uri", &self.redirect_uri)
            .field("post_logout_redirect_uri", &self.post_logout_redirect_uri)
            .field("required_role", &self.required_role)
            .field("role_claim", &self.role_claim)
            .finish()
    }
}

impl OidcConfig {
    /// All four core variables set => `Some`, none set => `None`, partial => error.
    fn from_env(base_url: &str) -> anyhow::Result<Option<Self>> {
        let var = |k: &str| env::var(k).ok().filter(|v| !v.trim().is_empty());
        let issuer_url = var("OIDC_ISSUER_URL");
        let client_id = var("OIDC_CLIENT_ID");
        let client_secret = var("OIDC_CLIENT_SECRET");
        let redirect_uri = var("OIDC_REDIRECT_URI");

        match (issuer_url, client_id, client_secret, redirect_uri) {
            (None, None, None, None) => Ok(None),
            (Some(issuer_url), Some(client_id), Some(client_secret), Some(redirect_uri)) => {
                Ok(Some(Self {
                    issuer_url,
                    client_id,
                    client_secret,
                    redirect_uri,
                    post_logout_redirect_uri: var("OIDC_POST_LOGOUT_REDIRECT_URI")
                        .unwrap_or_else(|| format!("{}/", base_url.trim_end_matches('/'))),
                    required_role: var("OIDC_REQUIRED_ROLE").unwrap_or_else(|| "vareni".into()),
                    role_claim: var("OIDC_ROLE_CLAIM").unwrap_or_else(|| "roles".into()),
                }))
            }
            _ => anyhow::bail!(
                "incomplete OIDC configuration: OIDC_ISSUER_URL, OIDC_CLIENT_ID, \
                 OIDC_CLIENT_SECRET and OIDC_REDIRECT_URI must all be set"
            ),
        }
    }
}
