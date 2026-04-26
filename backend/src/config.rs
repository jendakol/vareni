use std::env;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub anthropic_api_key: String,
    pub jwt_secret: String,
    pub jwt_expiry_hours: i64,
    pub base_url: String,
    pub push_notify_hour: u32,
    pub static_dir: String,
    pub upload_dir: String,
    pub vapid_public_key: String,
    pub vapid_private_key: String,
    pub vapid_contact: String,
    // Discovery
    pub embedding_model_dir: Option<String>,
    pub discovery_enabled: bool,
    // Log API (Home Assistant)
    pub log_api_token: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: env::var("DATABASE_URL")?,
            anthropic_api_key: env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
            jwt_secret: {
                let secret = env::var("JWT_SECRET")?;
                anyhow::ensure!(
                    secret.len() >= 32,
                    "JWT_SECRET must be at least 32 characters"
                );
                secret
            },
            jwt_expiry_hours: env::var("JWT_EXPIRY_HOURS")
                .unwrap_or_else(|_| "720".into())
                .parse()?,
            base_url: env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:8080".into()),
            push_notify_hour: env::var("PUSH_NOTIFY_HOUR")
                .unwrap_or_else(|_| "20".into())
                .parse()?,
            static_dir: env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into()),
            upload_dir: env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".into()),
            vapid_public_key: env::var("VAPID_PUBLIC_KEY").unwrap_or_default(),
            vapid_private_key: env::var("VAPID_PRIVATE_KEY").unwrap_or_default(),
            vapid_contact: env::var("VAPID_CONTACT")
                .unwrap_or_else(|_| "mailto:you@example.com".into()),
            embedding_model_dir: env::var("EMBEDDING_MODEL_DIR").ok(),
            discovery_enabled: env::var("DISCOVERY_ENABLED")
                .unwrap_or_else(|_| "true".into())
                .parse()
                .unwrap_or(true),
            log_api_token: env::var("LOG_API_TOKEN").ok(),
        })
    }
}
