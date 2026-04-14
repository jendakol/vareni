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
    StatusUpdateRequest, UpdateRecipeRequest,
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
    let sort = query.sort.as_deref().unwrap_or("recent");

    let status_str = query.status.as_deref().unwrap_or("saved,tested");
    let statuses: Vec<&str> = status_str.split(',').map(|s| s.trim()).collect();

    let (items, total) = db::recipes::list(
        &state.pool,
        query.q.as_deref(),
        query.tag.as_deref(),
        sort,
        page,
        per_page,
        &statuses,
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

pub async fn update_status(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<StatusUpdateRequest>,
) -> AppResult<Json<Recipe>> {
    const VALID_STATUSES: &[&str] = &[
        "discovered",
        "saved",
        "tested",
        "rejected",
        "rejected_similar",
    ];
    if !VALID_STATUSES.contains(&body.status.as_str()) {
        return Err(AppError::BadRequest(format!(
            "Invalid status: {}",
            body.status
        )));
    }

    match db::recipes::update_status(&state.pool, id, &body.status).await {
        Ok(Some(recipe)) => Ok(Json(recipe)),
        Ok(None) => Err(AppError::NotFound),
        Err(sqlx::Error::Protocol(msg)) if msg.contains("Invalid status transition") => {
            Err(AppError::Conflict(msg))
        }
        Err(e) => Err(AppError::Sqlx(e)),
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
