use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, patch, post, put};
use tokio::sync::Semaphore;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;

pub mod ai;
pub mod auth;
pub mod auth_oidc;
pub mod browser;
pub mod cli;
pub mod config;
pub mod db;
pub mod embedding;
pub mod error;
pub mod image_store;
pub mod images;
pub mod metrics;
pub mod models;
pub mod routes;
pub mod safe_fetch;
pub mod scraper;
pub mod validation;

/// Body cap for multipart routes carrying photos (per image: `images::MAX_INPUT_BYTES`).
const UPLOAD_BODY_LIMIT: usize = 100 * 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub config: Arc<config::Config>,
    pub http_client: reqwest::Client,
    pub embedding: Option<Arc<embedding::EmbeddingService>>,
    pub browser_semaphore: Arc<Semaphore>,
    pub image_semaphore: Arc<Semaphore>,
    pub image_fetch_semaphore: Arc<Semaphore>,
    pub oidc: Arc<auth_oidc::Oidc>,
    pub image_fetch: safe_fetch::ImageFetcher,
}

#[cfg(feature = "debug-auth")]
fn dev_routes() -> Router<AppState> {
    Router::new().route("/auth/dev-login", get(auth::dev_login))
}

#[cfg(not(feature = "debug-auth"))]
fn dev_routes() -> Router<AppState> {
    Router::new()
}

pub fn create_router(state: AppState) -> Router {
    let (prometheus_layer, prometheus_handle) = metrics::setup();

    let api = Router::new()
        // Auth
        .route("/auth/login", get(auth_oidc::login))
        .route("/auth/callback", get(auth_oidc::callback))
        .route("/auth/logout", post(auth_oidc::logout))
        .route("/auth/me", get(routes::auth::me))
        .route("/auth/users", get(routes::auth::list_users))
        .merge(dev_routes())
        // Recipes
        .route(
            "/recipes",
            get(routes::recipes::list).post(routes::recipes::create),
        )
        .route(
            "/recipes/{id}",
            get(routes::recipes::get)
                .put(routes::recipes::update)
                .delete(routes::recipes::delete),
        )
        .route(
            "/recipes/{id}/status",
            patch(routes::recipes::update_status),
        )
        .route(
            "/recipes/{id}/share",
            post(routes::recipes::share).delete(routes::recipes::unshare),
        )
        .route(
            "/recipes/{id}/images",
            post(routes::images::upload).layer(DefaultBodyLimit::max(UPLOAD_BODY_LIMIT)),
        )
        .route(
            "/recipes/{id}/images/{image_id}",
            axum::routing::delete(routes::images::delete),
        )
        .route(
            "/recipes/{id}/images/{image_id}/{variant}",
            get(routes::images::serve),
        )
        // Discovery
        .route("/discover", post(routes::discover::discover))
        // Ingestion
        .route(
            "/ingest",
            post(routes::ingest::ingest).layer(DefaultBodyLimit::max(UPLOAD_BODY_LIMIT)),
        )
        // Chat
        .route("/chat/{recipe_id}", post(routes::chat::chat))
        // Meal plan (suggest BEFORE {id} to avoid conflict)
        .route("/plan/suggest", post(routes::plan::suggest))
        .route(
            "/plan/suggest_free_text",
            get(routes::plan::suggest_free_text),
        )
        .route("/plan/history", get(routes::plan::history))
        .route("/plan", get(routes::plan::list).post(routes::plan::create))
        .route(
            "/plan/{id}",
            put(routes::plan::update).delete(routes::plan::delete),
        )
        // Settings
        .route(
            "/settings/restrictions",
            post(routes::settings::add_restriction).delete(routes::settings::remove_restriction),
        )
        .route(
            "/settings/preferences",
            post(routes::settings::add_preference).delete(routes::settings::remove_preference),
        )
        // Public
        .route(
            "/public/recipes/{slug}",
            get(routes::public::get_recipe_by_slug),
        )
        .route(
            "/public/recipes/{slug}/cover/{variant}",
            get(routes::public::get_cover),
        )
        // Log API (Home Assistant)
        .route("/log", post(routes::log::create_entry));

    let static_dir = state.config.static_dir.clone();

    Router::new()
        .nest("/api", api)
        .route(
            "/metrics",
            get(move || async move { prometheus_handle.render() }),
        )
        .fallback_service(
            ServeDir::new(&static_dir).fallback(ServeFile::new(format!("{static_dir}/index.html"))),
        )
        .layer(prometheus_layer)
        .layer(
            TraceLayer::new_for_http()
                // Path only: query strings may carry OIDC code/state.
                .make_span_with(|req: &axum::http::Request<axum::body::Body>| {
                    tracing::info_span!("request", method = %req.method(), path = %req.uri().path())
                })
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state)
}
