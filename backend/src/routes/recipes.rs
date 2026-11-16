use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthUser;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::metrics::RECIPE_STATUS_CHANGES_TOTAL;
use crate::models::{
    CreateRecipeRequest, Paginated, Recipe, RecipeDetail, RecipeListQuery, ShareResponse,
    StatusUpdateRequest, UpdateRecipeRequest,
};

pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateRecipeRequest>,
) -> AppResult<(StatusCode, Json<RecipeDetail>)> {
    // source_url is validated in db::recipes::create — the actual write sink.
    let mut recipe = db::recipes::create(&state.pool, auth.user_id, &body).await?;
    if let Some(url) = body.cover_image_url.as_deref().filter(|u| !u.is_empty()) {
        crate::image_store::attach_cover_from_url(&state, recipe.recipe.id, url).await;
        if let Some(fresh) = db::recipes::get_by_id(&state.pool, recipe.recipe.id).await? {
            recipe = fresh;
        }
    }
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
    // source_url is validated in db::recipes::update — the actual write sink.
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
        crate::image_store::remove_recipe_dir(&state.config.upload_dir, id).await;
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
        Ok(Some((from_status, recipe))) => {
            metrics::counter!(
                RECIPE_STATUS_CHANGES_TOTAL,
                "from_status" => from_status,
                "to_status" => body.status.clone(),
            )
            .increment(1);
            Ok(Json(recipe))
        }
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
    use rand::Rng;
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let base: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>();
    let base = base.trim_matches('-').replace("--", "-");
    let base: String = base.chars().take(40).collect();
    let base = base.trim_end_matches('-');
    let mut rng = rand::rng();
    let suffix: String = (0..8)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect();
    format!("{base}-{suffix}")
}

#[cfg(test)]
mod tests {
    use super::generate_slug;

    #[test]
    fn slug_truncates_multibyte_title_without_panic() {
        let s = generate_slug(&"čřžýáíé".repeat(10));
        assert!(s.starts_with("čřžýáíé"));
    }

    #[test]
    fn slug_truncates_at_mid_char_byte_boundary() {
        let s = generate_slug(&format!("a{}", "č".repeat(30)));
        assert!(s.starts_with("ačč"));
    }

    #[test]
    fn slug_suffix_is_8_base36_chars() {
        let s = generate_slug("Svíčková");
        let suffix = s.rsplit('-').next().unwrap();
        assert_eq!(suffix.len(), 8);
        assert!(
            suffix
                .chars()
                .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase())
        );
    }
}
