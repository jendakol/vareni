use axum::Json;
use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use bytes::{Bytes, BytesMut};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthUser;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::image_store::{self, Variant};
use crate::images::{self, ImageError, MAX_INPUT_BYTES};
use crate::models::{ImageKind, RecipeImage};

fn bad(e: impl std::fmt::Display) -> AppError {
    AppError::BadRequest(e.to_string())
}

/// Reads a field in chunks, aborting as soon as it exceeds the per-image cap.
pub async fn read_capped(field: axum::extract::multipart::Field<'_>) -> AppResult<Bytes> {
    read_capped_to(field, MAX_INPUT_BYTES, || ImageError::TooLarge.into()).await
}

const MAX_KIND_BYTES: usize = 32;

async fn read_capped_to(
    mut field: axum::extract::multipart::Field<'_>,
    cap: usize,
    over: fn() -> AppError,
) -> AppResult<Bytes> {
    let mut buf = BytesMut::new();
    while let Some(chunk) = field.chunk().await.map_err(bad)? {
        if buf.len() + chunk.len() > cap {
            return Err(over());
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf.freeze())
}

pub async fn upload(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(recipe_id): Path<Uuid>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<Vec<RecipeImage>>)> {
    if !db::recipes::exists(&state.pool, recipe_id).await? {
        return Err(AppError::NotFound);
    }

    let kind_field = multipart
        .next_field()
        .await
        .map_err(bad)?
        .filter(|f| f.name() == Some("kind"))
        .ok_or_else(|| AppError::BadRequest("Pole kind musí být první".into()))?;
    let kind_bytes = read_capped_to(kind_field, MAX_KIND_BYTES, || {
        AppError::BadRequest("Neplatný druh obrázku".into())
    })
    .await?;
    let kind = std::str::from_utf8(&kind_bytes)
        .ok()
        .and_then(ImageKind::parse)
        .ok_or_else(|| AppError::BadRequest("Neplatný druh obrázku".into()))?;

    let mut stored = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(bad)? {
        if field.name() != Some("image") {
            return Err(AppError::BadRequest("Neočekávané pole formuláře".into()));
        }
        let data = read_capped(field).await?;
        let processed = images::process_async(&state.image_semaphore, data, kind).await?;
        if kind == ImageKind::Cover {
            // Check before storing: a rejected request must not replace the cover.
            if multipart.next_field().await.map_err(bad)?.is_some() {
                return Err(AppError::BadRequest(
                    "Úvodní fotka může být jen jedna".into(),
                ));
            }
            stored.push(image_store::store(&state, recipe_id, kind, processed).await?);
            break;
        }
        stored.push(image_store::store(&state, recipe_id, kind, processed).await?);
    }

    if stored.is_empty() {
        return Err(AppError::BadRequest(
            "Nahrajte alespoň jeden obrázek".into(),
        ));
    }
    Ok((StatusCode::CREATED, Json(stored)))
}

pub async fn delete(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path((recipe_id, image_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    if image_store::delete(&state, recipe_id, image_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

pub async fn serve(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path((recipe_id, image_id, variant)): Path<(Uuid, Uuid, String)>,
) -> AppResult<Response> {
    let variant = Variant::parse(&variant).ok_or(AppError::NotFound)?;
    db::images::get(&state.pool, recipe_id, image_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let path = image_store::variant_path(&state.config.upload_dir, recipe_id, image_id, variant);
    let file = tokio::fs::File::open(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound
        } else {
            AppError::Internal(e.into())
        }
    })?;

    Response::builder()
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header(
            header::CACHE_CONTROL,
            "private, max-age=31536000, immutable",
        )
        .body(Body::from_stream(ReaderStream::new(file)))
        .map_err(|e| AppError::Internal(e.into()))
}
