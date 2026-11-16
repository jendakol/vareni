use std::sync::Arc;

use sqlx::PgPool;
use tracing_subscriber::EnvFilter;

use cooking_app::{AppState, auth_oidc, cli, config, create_router, db, metrics};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Try .env in current dir, then parent (for running from backend/)
    if dotenvy::dotenv().is_err() {
        dotenvy::from_filename("../.env").ok();
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new("cooking_app=debug,tower_http=info,chromiumoxide=error,warn")
        }))
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        return cli::run(&args).await;
    }

    let config = config::Config::from_env()?;
    match &config.oidc {
        Some(oidc) => tracing::info!(issuer = %oidc.issuer_url, "OIDC login enabled"),
        None => tracing::warn!("OIDC not configured — web login disabled, all sessions rejected"),
    }
    let pool = PgPool::connect(&config.database_url).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    metrics::spawn_gauge_refresh(pool.clone(), config.metrics_gauge_refresh_secs);
    db::sessions::spawn_cleanup(pool.clone());

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
    default_headers.insert(
        "Sec-Ch-Ua",
        "\"Chromium\";v=\"146\", \"Not-A.Brand\";v=\"24\", \"Google Chrome\";v=\"146\""
            .parse()
            .unwrap(),
    );
    default_headers.insert("Sec-Ch-Ua-Mobile", "?0".parse().unwrap());
    default_headers.insert("Sec-Ch-Ua-Platform", "\"Linux\"".parse().unwrap());
    default_headers.insert("Upgrade-Insecure-Requests", "1".parse().unwrap());

    let http_client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36")
        .default_headers(default_headers)
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    // Load ONNX model on a blocking thread to avoid async runtime deadlock (spec section 8.3)
    let embedding = if config.discovery_enabled {
        match &config.embedding_model_dir {
            Some(dir) => {
                let dir_owned = dir.clone();
                match tokio::task::spawn_blocking(move || {
                    cooking_app::embedding::EmbeddingService::new(&dir_owned)
                })
                .await
                .expect("blocking task panicked")
                {
                    Ok(svc) => {
                        tracing::info!(model_dir = %dir, "ONNX embedding model loaded — discovery enabled");
                        Some(Arc::new(svc))
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load embedding model from {dir}: {e} — discovery disabled"
                        );
                        None
                    }
                }
            }
            None => {
                tracing::info!("EMBEDDING_MODEL_DIR not set — discovery disabled");
                None
            }
        }
    } else {
        tracing::info!("Discovery disabled via DISCOVERY_ENABLED=false");
        None
    };

    std::fs::create_dir_all(std::path::Path::new(&config.upload_dir).join("recipes"))?;

    let state = AppState {
        pool,
        config: Arc::new(config),
        http_client,
        embedding,
        browser_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        image_semaphore: Arc::new(tokio::sync::Semaphore::new(2)),
        image_fetch_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
        oidc: Arc::new(auth_oidc::Oidc::new()?),
        image_fetch: cooking_app::safe_fetch::ImageFetcher::new(false)?,
    };

    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("Listening on 0.0.0.0:8080");
    axum::serve(listener, app).await?;

    Ok(())
}
