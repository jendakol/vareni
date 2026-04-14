use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post, put};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

pub mod ai;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod models;
pub mod push_notifier;
pub mod routes;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub config: Arc<config::Config>,
    pub http_client: reqwest::Client,
}

pub fn create_router(state: AppState) -> Router {
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
            "/recipes/{id}/share",
            post(routes::recipes::share).delete(routes::recipes::unshare),
        )
        // Ingestion
        .route("/ingest", post(routes::ingest::ingest))
        // Chat
        .route("/chat/{recipe_id}", post(routes::chat::chat))
        // Meal plan (suggest BEFORE {id} to avoid conflict)
        .route("/plan/suggest", post(routes::plan::suggest))
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
        );

    let static_dir = state.config.static_dir.clone();
    let upload_dir = state.config.upload_dir.clone();

    Router::new()
        .nest("/api", api)
        .nest_service("/uploads", ServeDir::new(&upload_dir))
        .fallback_service(
            ServeDir::new(&static_dir).fallback(ServeFile::new(format!("{static_dir}/index.html"))),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
