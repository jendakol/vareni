use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use tokio_util::io::ReaderStream;

use crate::AppState;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::image_store::{self, Variant};
use crate::models::PublicRecipe;

pub async fn get_recipe_by_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Json<PublicRecipe>> {
    let recipe = db::recipes::get_by_slug(&state.pool, &slug)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(recipe.into()))
}

/// True if an `If-None-Match` header matches `etag` (weak prefix, lists and `*` tolerated).
fn etag_matches(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .any(|t| t == "*" || t.strip_prefix("W/").unwrap_or(t) == etag)
}

pub async fn get_cover(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((slug, variant)): Path<(String, String)>,
) -> AppResult<Response> {
    let variant = Variant::parse(&variant).ok_or(AppError::NotFound)?;
    let (recipe_id, image_id) = db::images::public_cover(&state.pool, &slug)
        .await?
        .ok_or(AppError::NotFound)?;
    let etag = format!("\"{image_id}-{}\"", variant.as_str());

    let builder = Response::builder()
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::ETAG, &etag);
    if etag_matches(&headers, &etag) {
        return builder
            .status(StatusCode::NOT_MODIFIED)
            .body(Body::empty())
            .map_err(|e| AppError::Internal(e.into()));
    }

    let path = image_store::variant_path(&state.config.upload_dir, recipe_id, image_id, variant);
    let file = tokio::fs::File::open(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound
        } else {
            AppError::Internal(e.into())
        }
    })?;
    builder
        .header(header::CONTENT_TYPE, "image/jpeg")
        .body(Body::from_stream(ReaderStream::new(file)))
        .map_err(|e| AppError::Internal(e.into()))
}
