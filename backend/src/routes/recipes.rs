use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthUser;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::{
    CreateRecipeRequest, Paginated, Recipe, RecipeDetail, RecipeListQuery, ShareResponse,
    UpdateRecipeRequest,
};

pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateRecipeRequest>,
) -> AppResult<(StatusCode, Json<RecipeDetail>)> {
    let recipe = db::recipes::create(&state.pool, auth.user_id, &body).await?;
    Ok((StatusCode::CREATED, Json(recipe)))
}

pub async fn list(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(query): Query<RecipeListQuery>,
) -> AppResult<Json<Paginated<Recipe>>> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    let (items, total) = db::recipes::list(
        &state.pool,
        query.q.as_deref(),
        query.tag.as_deref(),
        page,
        per_page,
    )
    .await?;

    Ok(Json(Paginated {
        items,
        total,
        page,
        per_page,
    }))
}

pub async fn get(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<RecipeDetail>> {
    let recipe = db::recipes::get_by_id(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(recipe))
}

pub async fn update(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRecipeRequest>,
) -> AppResult<Json<RecipeDetail>> {
    let recipe = db::recipes::update(&state.pool, id, &body)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(recipe))
}

pub async fn delete(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let deleted = db::recipes::delete(&state.pool, id).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

pub async fn share(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ShareResponse>> {
    // Check recipe exists
    let detail = db::recipes::get_by_id(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;

    // If already shared, return existing slug
    if let Some(slug) = &detail.recipe.public_slug {
        return Ok(Json(ShareResponse {
            share_url: format!("{}/r/{}", state.config.base_url, slug),
            slug: slug.clone(),
        }));
    }

    // Generate slug: lowercase title + random suffix
    let slug = generate_slug(&detail.recipe.title);
    db::recipes::set_public_slug(&state.pool, id, &slug).await?;

    Ok(Json(ShareResponse {
        share_url: format!("{}/r/{}", state.config.base_url, slug),
        slug,
    }))
}

pub async fn unshare(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    db::recipes::remove_public_slug(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn generate_slug(title: &str) -> String {
    let base: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>();
    let base = base.trim_matches('-').replace("--", "-");
    let suffix: u32 = rand::random::<u32>() % 10000;
    format!("{}-{suffix:04}", &base[..base.len().min(40)])
}
