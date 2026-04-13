use std::sync::Arc;

use sqlx::PgPool;
use tracing_subscriber::EnvFilter;

use cooking_app::{AppState, config, create_router, push_notifier};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Try .env in current dir, then parent (for running from backend/)
    if dotenvy::dotenv().is_err() {
        dotenvy::from_filename("../.env").ok();
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("cooking_app=debug,tower_http=info,warn")),
        )
        .init();

    let config = config::Config::from_env()?;
    let pool = PgPool::connect(&config.database_url).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    push_notifier::start_notifier(pool.clone(), config.push_notify_hour);

    let state = AppState {
        pool,
        config: Arc::new(config),
    };

    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("Listening on 0.0.0.0:8080");
    axum::serve(listener, app).await?;

    Ok(())
}
