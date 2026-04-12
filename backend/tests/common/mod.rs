use axum::Router;
use sqlx::PgPool;
use std::sync::Arc;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

pub struct TestContext {
    pub pool: PgPool,
    pub router: Router,
    pub user1_token: String,
    pub user1_id: uuid::Uuid,
    pub user2_token: String,
    pub user2_id: uuid::Uuid,
    _container: ContainerAsync<GenericImage>,
}

impl TestContext {
    pub async fn new() -> Self {
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

        // Seed two test users
        let password_hash = bcrypt::hash("testpass123", 4).expect("Failed to hash");

        let user1 = sqlx::query_as::<_, (uuid::Uuid,)>(
            "INSERT INTO users (name, email, password_hash) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind("Test User 1")
        .bind("user1@test.com")
        .bind(&password_hash)
        .fetch_one(&pool)
        .await
        .expect("Failed to seed user1");

        let user2 = sqlx::query_as::<_, (uuid::Uuid,)>(
            "INSERT INTO users (name, email, password_hash) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind("Test User 2")
        .bind("user2@test.com")
        .bind(&password_hash)
        .fetch_one(&pool)
        .await
        .expect("Failed to seed user2");

        let config = cooking_app::config::Config {
            database_url,
            anthropic_api_key: String::new(),
            jwt_secret: "test-secret-key-for-testing-12345".into(),
            jwt_expiry_hours: 24,
            base_url: "http://localhost:8080".into(),
            push_notify_hour: 20,
            static_dir: "./static".into(),
            upload_dir: std::env::temp_dir()
                .join(format!("cooking-test-{}", uuid::Uuid::new_v4()))
                .to_string_lossy()
                .into_owned(),
            vapid_public_key: String::new(),
            vapid_private_key: String::new(),
            vapid_contact: "mailto:test@test.com".into(),
        };

        let user1_token =
            cooking_app::auth::encode_jwt(user1.0, &config.jwt_secret, config.jwt_expiry_hours)
                .expect("Failed to encode JWT");
        let user2_token =
            cooking_app::auth::encode_jwt(user2.0, &config.jwt_secret, config.jwt_expiry_hours)
                .expect("Failed to encode JWT");

        let state = cooking_app::AppState {
            pool: pool.clone(),
            config: Arc::new(config),
        };
        let router = cooking_app::create_router(state);

        Self {
            pool,
            router,
            user1_token,
            user1_id: user1.0,
            user2_token,
            user2_id: user2.0,
            _container: container,
        }
    }

    /// Build a request with auth header for user 1
    pub fn auth_header_1(&self) -> (String, String) {
        (
            "Authorization".into(),
            format!("Bearer {}", self.user1_token),
        )
    }

    /// Build a request with auth header for user 2
    pub fn auth_header_2(&self) -> (String, String) {
        (
            "Authorization".into(),
            format!("Bearer {}", self.user2_token),
        )
    }
}
