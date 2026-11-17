use axum::Router;
use sqlx::PgPool;
use std::sync::Arc;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

pub const USER1_SUB: &str = "sub-user-1";
pub const USER2_SUB: &str = "sub-user-2";
/// Nothing listens here, so lazy discovery fails fast in tests that don't mock the IdP.
pub const UNREACHABLE_ISSUER: &str = "http://127.0.0.1:1";

/// Session cookie + CSRF token of a seeded user.
#[allow(dead_code)]
#[derive(Clone)]
pub struct TestAuth {
    pub cookie: String,
    pub csrf: String,
}

#[allow(dead_code)]
pub trait WithAuth {
    /// Adds the session cookie and the CSRF header (harmless on GET).
    fn with_auth(self, auth: &TestAuth) -> Self;
}

impl WithAuth for axum::http::request::Builder {
    fn with_auth(self, auth: &TestAuth) -> Self {
        self.header("Cookie", &auth.cookie)
            .header("X-CSRF-Token", &auth.csrf)
    }
}

#[allow(dead_code)]
pub struct TestContext {
    pub pool: PgPool,
    pub router: Router,
    pub user1_id: uuid::Uuid,
    pub user1_auth: TestAuth,
    pub user2_id: uuid::Uuid,
    pub user2_auth: TestAuth,
    pub config: cooking_app::config::Config,
    _container: ContainerAsync<GenericImage>,
}

impl TestContext {
    #[allow(dead_code)]
    pub async fn new() -> Self {
        Self::with_config(|_| {}).await
    }

    /// Context whose OIDC issuer points at `issuer_url` (e.g. a wiremock server).
    #[allow(dead_code)]
    pub async fn with_issuer(issuer_url: &str) -> Self {
        let issuer_url = issuer_url.to_owned();
        Self::with_config(move |c| c.oidc.as_mut().unwrap().issuer_url = issuer_url).await
    }

    #[allow(dead_code)]
    pub async fn with_config(customize: impl FnOnce(&mut cooking_app::config::Config)) -> Self {
        let image = GenericImage::new("pgvector/pgvector", "pg18")
            .with_wait_for(testcontainers::core::WaitFor::message_on_stderr(
                "database system is ready to accept connections",
            ))
            .with_exposed_port(testcontainers::core::ContainerPort::Tcp(5432))
            .with_env_var("POSTGRES_DB", "test")
            .with_env_var("POSTGRES_USER", "test")
            .with_env_var("POSTGRES_PASSWORD", "test");

        let container = image.start().await.expect("Failed to start postgres");
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let database_url = format!("postgresql://test:test@127.0.0.1:{port}/test");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test DB");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        // Seed two test users linked to IdP subjects
        let user1 = seed_user(&pool, "Test User 1", "user1@test.com", USER1_SUB).await;
        let user2 = seed_user(&pool, "Test User 2", "user2@test.com", USER2_SUB).await;

        let mut config = cooking_app::config::Config {
            database_url,
            anthropic_api_key: String::new(),
            environment: "test".into(),
            cookie_secure: true,
            session_ttl_hours: 24,
            oidc: Some(cooking_app::config::OidcConfig {
                issuer_url: UNREACHABLE_ISSUER.into(),
                client_id: "vareni".into(),
                client_secret: "test-client-secret".into(),
                redirect_uri: "http://localhost:8080/api/auth/callback".into(),
                post_logout_redirect_uri: "http://localhost:8080/".into(),
                required_role: "vareni".into(),
                role_claim: "roles".into(),
            }),
            base_url: "http://localhost:8080".into(),
            static_dir: "./static".into(),
            upload_dir: std::env::temp_dir()
                .join(format!("vareni-test-{}", uuid::Uuid::new_v4()))
                .to_string_lossy()
                .into_owned(),
            ai_models: Default::default(),
            embedding_model_dir: None,
            discovery_enabled: false,
            log_api_token: Some("test-log-token".into()),
            metrics_gauge_refresh_secs: 60,
            image_fetch_allow_private: false,
        };

        customize(&mut config);

        let user1_auth = create_session(&pool, user1, 24).await;
        let user2_auth = create_session(&pool, user2, 24).await;

        let router = build_router(&pool, &config);

        Self {
            pool,
            router,
            user1_id: user1,
            user1_auth,
            user2_id: user2,
            user2_auth,
            config,
            _container: container,
        }
    }

    /// Session auth for user 1
    #[allow(dead_code)]
    pub fn auth_1(&self) -> TestAuth {
        self.user1_auth.clone()
    }

    /// Session auth for user 2
    #[allow(dead_code)]
    pub fn auth_2(&self) -> TestAuth {
        self.user2_auth.clone()
    }
}

/// Router over the same DB with a (possibly modified) config.
#[allow(dead_code)]
pub fn build_router(pool: &PgPool, config: &cooking_app::config::Config) -> Router {
    let state = cooking_app::AppState {
        pool: pool.clone(),
        config: Arc::new(config.clone()),
        http_client: reqwest::Client::new(),
        embedding: None, // Discovery disabled in tests
        browser_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        image_semaphore: Arc::new(tokio::sync::Semaphore::new(2)),
        image_fetch_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
        oidc: Arc::new(cooking_app::auth_oidc::Oidc::new().expect("OIDC client")),
        image_fetch: cooking_app::safe_fetch::ImageFetcher::new(config.image_fetch_allow_private)
            .expect("image fetcher"),
    };
    cooking_app::create_router(state)
}

#[allow(dead_code)]
pub async fn seed_user(pool: &PgPool, name: &str, email: &str, sub: &str) -> uuid::Uuid {
    sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO users (name, email, external_id) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(name)
    .bind(email)
    .bind(sub)
    .fetch_one(pool)
    .await
    .expect("Failed to seed user")
}

/// Real session row; negative `ttl_hours` creates an already expired session.
#[allow(dead_code)]
pub async fn create_session(pool: &PgPool, user_id: uuid::Uuid, ttl_hours: i64) -> TestAuth {
    let session = cooking_app::db::sessions::create(pool, user_id, None, None, ttl_hours)
        .await
        .expect("Failed to create session");
    TestAuth {
        cookie: format!("{}={}", cooking_app::auth::SESSION_COOKIE, session.raw_id),
        csrf: session.csrf_token,
    }
}
