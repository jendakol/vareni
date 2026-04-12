use axum::Json;
use axum::extract::{Path, State};

use crate::AppState;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::RecipeDetail;

pub async fn get_recipe_by_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Json<RecipeDetail>> {
    let recipe = db::recipes::get_by_slug(&state.pool, &slug)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(recipe))
}
