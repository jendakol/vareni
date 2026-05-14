use std::sync::Arc;

use axum::Router;
use axum::routing::{get, patch, post, put};
use tokio::sync::Semaphore;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

pub mod ai;
pub mod auth;
pub mod browser;
pub mod config;
pub mod db;
pub mod embedding;
pub mod error;
pub mod metrics;
pub mod models;
pub mod push_notifier;
pub mod routes;
pub mod scraper;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub config: Arc<config::Config>,
    pub http_client: reqwest::Client,
    pub embedding: Option<Arc<embedding::EmbeddingService>>,
    pub browser_semaphore: Arc<Semaphore>,
}

pub fn create_router(state: AppState) -> Router {
    let (prometheus_layer, prometheus_handle) = metrics::setup();

    let api = Router::new()
        // Auth
        .route("/auth/login", post(routes::auth::login))
        .route("/auth/me", get(routes::auth::me))
        .route("/auth/users", get(routes::auth::list_users))
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
        // Discovery
        .route("/discover", post(routes::discover::discover))
        // Ingestion
        .route("/ingest", post(routes::ingest::ingest))
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
        // Push
        .route("/push/subscribe", post(routes::push::subscribe))
        .route("/push/unsubscribe", post(routes::push::unsubscribe))
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
        // Log API (Home Assistant)
        .route("/log", post(routes::log::create_entry));

    let static_dir = state.config.static_dir.clone();
    let upload_dir = state.config.upload_dir.clone();

    Router::new()
        .nest("/api", api)
        .route(
            "/metrics",
            get(move || async move { prometheus_handle.render() }),
        )
        .nest_service("/uploads", ServeDir::new(&upload_dir))
        .fallback_service(
            ServeDir::new(&static_dir).fallback(ServeFile::new(format!("{static_dir}/index.html"))),
        )
        .layer(prometheus_layer)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(
                    DefaultMakeSpan::new()
                        .level(Level::INFO)
                        .include_headers(false),
                )
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state)
}
