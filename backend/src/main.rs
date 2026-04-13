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

    let mut default_headers = reqwest::header::HeaderMap::new();
    default_headers.insert(
        reqwest::header::ACCEPT,
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
            .parse()
            .unwrap(),
    );
    default_headers.insert(
        reqwest::header::ACCEPT_LANGUAGE,
        "cs,en;q=0.5".parse().unwrap(),
    );
    default_headers.insert("Sec-Fetch-Dest", "document".parse().unwrap());
    default_headers.insert("Sec-Fetch-Mode", "navigate".parse().unwrap());
    default_headers.insert("Sec-Fetch-Site", "none".parse().unwrap());
    default_headers.insert("Sec-Fetch-User", "?1".parse().unwrap());
    default_headers.insert("Upgrade-Insecure-Requests", "1".parse().unwrap());

    let http_client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36")
        .default_headers(default_headers)
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let state = AppState {
        pool,
        config: Arc::new(config),
        http_client,
    };

    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("Listening on 0.0.0.0:8080");
    axum::serve(listener, app).await?;

    Ok(())
}
