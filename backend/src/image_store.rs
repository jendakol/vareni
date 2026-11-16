//! On-disk storage for recipe images: `{upload_dir}/recipes/{recipe_id}/{image_id}_{variant}.jpg`.

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::AppState;
use crate::db;
use crate::error::AppError;
use crate::images::Processed;
use crate::models::{ImageKind, RecipeImage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Full,
    Thumb,
}

impl Variant {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "full" => Some(Self::Full),
            "thumb" => Some(Self::Thumb),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Thumb => "thumb",
        }
    }
}

fn recipe_dir(upload_dir: &str, recipe_id: Uuid) -> PathBuf {
    Path::new(upload_dir)
        .join("recipes")
        .join(recipe_id.to_string())
}

pub fn variant_path(
    upload_dir: &str,
    recipe_id: Uuid,
    image_id: Uuid,
    variant: Variant,
) -> PathBuf {
    recipe_dir(upload_dir, recipe_id).join(format!("{image_id}_{}.jpg", variant.as_str()))
}

fn io_err(e: std::io::Error) -> AppError {
    AppError::Internal(e.into())
}

async fn remove_file_quiet(path: &Path) {
    if let Err(e) = tokio::fs::remove_file(path).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("Failed to remove {}: {e}", path.display());
    }
}

async fn remove_variants(upload_dir: &str, recipe_id: Uuid, image_id: Uuid) {
    for v in [Variant::Full, Variant::Thumb] {
        remove_file_quiet(&variant_path(upload_dir, recipe_id, image_id, v)).await;
    }
}

fn tmp_of(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".tmp");
    PathBuf::from(s)
}

/// Writes the files and the DB row. Final files only become visible under the new,
/// still-unreachable UUID, so renaming before commit is safe.
/// Runs detached, so a dropped caller (client disconnect) cannot leave it half done.
pub async fn store(
    state: &AppState,
    recipe_id: Uuid,
    kind: ImageKind,
    processed: Processed,
) -> Result<RecipeImage, AppError> {
    let state = state.clone();
    tokio::spawn(async move { store_task(&state, recipe_id, kind, processed).await })
        .await
        .map_err(|e| AppError::Internal(e.into()))?
}

async fn store_task(
    state: &AppState,
    recipe_id: Uuid,
    kind: ImageKind,
    processed: Processed,
) -> Result<RecipeImage, AppError> {
    let dir = state.config.upload_dir.as_str();
    let image_id = Uuid::new_v4();
    tokio::fs::create_dir_all(recipe_dir(dir, recipe_id))
        .await
        .map_err(io_err)?;

    let result = store_inner(state, recipe_id, image_id, kind, &processed).await;
    match result {
        Ok((image, old_cover)) => {
            if let Some(old) = old_cover {
                remove_variants(dir, recipe_id, old).await;
            }
            Ok(image)
        }
        Err(e) => {
            for v in [Variant::Full, Variant::Thumb] {
                let path = variant_path(dir, recipe_id, image_id, v);
                remove_file_quiet(&tmp_of(&path)).await;
                remove_file_quiet(&path).await;
            }
            Err(e)
        }
    }
}

async fn store_inner(
    state: &AppState,
    recipe_id: Uuid,
    image_id: Uuid,
    kind: ImageKind,
    processed: &Processed,
) -> Result<(RecipeImage, Option<Uuid>), AppError> {
    let dir = state.config.upload_dir.as_str();
    let targets = [
        (Variant::Full, &processed.full),
        (Variant::Thumb, &processed.thumb),
    ];
    for (v, data) in targets {
        let path = variant_path(dir, recipe_id, image_id, v);
        tokio::fs::write(tmp_of(&path), data)
            .await
            .map_err(io_err)?;
    }

    let mut tx = state.pool.begin().await?;
    let old_cover = if kind == ImageKind::Cover {
        let old = db::images::lock_cover(&mut tx, recipe_id).await?;
        if let Some(old) = old {
            db::images::delete(&mut tx, recipe_id, old).await?;
        }
        old
    } else {
        None
    };

    let image = db::images::insert(
        &mut tx,
        image_id,
        recipe_id,
        kind,
        processed.width as i32,
        processed.height as i32,
    )
    .await
    .map_err(|e| match e.as_database_error() {
        // Recipe deleted while the upload was in flight.
        Some(db) if db.is_foreign_key_violation() => AppError::NotFound,
        _ => AppError::Sqlx(e),
    })?;

    for (v, _) in targets {
        let path = variant_path(dir, recipe_id, image_id, v);
        tokio::fs::rename(tmp_of(&path), &path)
            .await
            .map_err(io_err)?;
    }
    tx.commit().await?;
    Ok((image, old_cover))
}

/// Deletes the row first, then the files best-effort. Returns whether the image existed.
pub async fn delete(state: &AppState, recipe_id: Uuid, image_id: Uuid) -> Result<bool, AppError> {
    let mut conn = state.pool.acquire().await?;
    let deleted = db::images::delete(&mut conn, recipe_id, image_id).await?;
    drop(conn);
    if deleted.is_none() {
        return Ok(false);
    }
    remove_variants(&state.config.upload_dir, recipe_id, image_id).await;
    Ok(true)
}

pub async fn remove_recipe_dir(upload_dir: &str, recipe_id: Uuid) {
    if let Err(e) = tokio::fs::remove_dir_all(recipe_dir(upload_dir, recipe_id)).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("Failed to remove image dir of recipe {recipe_id}: {e}");
    }
}

/// Max wait for a fetch slot, so a busy queue never stalls a synchronous create.
const FETCH_QUEUE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Best-effort cover import from a remote URL; every failure is logged and swallowed.
/// The fetch permit is held until processing has taken the bytes, bounding buffered memory.
pub async fn attach_cover_from_url(state: &AppState, recipe_id: Uuid, url: &str) {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default();
    let permit = match tokio::time::timeout(
        FETCH_QUEUE_TIMEOUT,
        state.image_fetch_semaphore.acquire(),
    )
    .await
    {
        Ok(Ok(p)) => p,
        Ok(Err(_)) => {
            tracing::warn!(%recipe_id, host, "cover fetch semaphore closed");
            return;
        }
        Err(_) => {
            tracing::warn!(%recipe_id, host, "cover fetch skipped: queue wait timed out");
            return;
        }
    };
    let bytes = match state.image_fetch.fetch_image(url).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(%recipe_id, host, error = ?e, "cover fetch failed");
            return;
        }
    };
    let processed =
        match crate::images::process_async(&state.image_semaphore, bytes, ImageKind::Cover).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(%recipe_id, host, error = ?e, "cover processing failed");
                return;
            }
        };
    drop(permit);
    if let Err(e) = store(state, recipe_id, ImageKind::Cover, processed).await {
        tracing::warn!(%recipe_id, host, error = ?e, "cover store failed");
    }
}
