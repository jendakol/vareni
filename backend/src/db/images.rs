use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::models::{ImageKind, ImageRef, RecipeImage};

const COLUMNS: &str = "id, recipe_id, kind, width, height, created_at";

pub async fn insert(
    conn: &mut PgConnection,
    id: Uuid,
    recipe_id: Uuid,
    kind: ImageKind,
    width: i32,
    height: i32,
) -> Result<RecipeImage, sqlx::Error> {
    sqlx::query_as(&format!(
        "INSERT INTO recipe_images (id, recipe_id, kind, width, height)
         VALUES ($1, $2, $3, $4, $5) RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(recipe_id)
    .bind(kind.as_str())
    .bind(width)
    .bind(height)
    .fetch_one(conn)
    .await
}

pub async fn get(
    pool: &PgPool,
    recipe_id: Uuid,
    image_id: Uuid,
) -> Result<Option<RecipeImage>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM recipe_images WHERE id = $1 AND recipe_id = $2"
    ))
    .bind(image_id)
    .bind(recipe_id)
    .fetch_optional(pool)
    .await
}

/// Locks the recipe row and returns its current cover id, serializing cover replacement.
pub async fn lock_cover(
    conn: &mut PgConnection,
    recipe_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query("SELECT id FROM recipes WHERE id = $1 FOR UPDATE")
        .bind(recipe_id)
        .fetch_optional(&mut *conn)
        .await?;
    sqlx::query_scalar("SELECT id FROM recipe_images WHERE recipe_id = $1 AND kind = 'cover'")
        .bind(recipe_id)
        .fetch_optional(conn)
        .await
}

pub async fn delete(
    conn: &mut PgConnection,
    recipe_id: Uuid,
    image_id: Uuid,
) -> Result<Option<RecipeImage>, sqlx::Error> {
    sqlx::query_as(&format!(
        "DELETE FROM recipe_images WHERE id = $1 AND recipe_id = $2 RETURNING {COLUMNS}"
    ))
    .bind(image_id)
    .bind(recipe_id)
    .fetch_optional(conn)
    .await
}

pub async fn list_screenshots(
    pool: &PgPool,
    recipe_id: Uuid,
) -> Result<Vec<ImageRef>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, width, height FROM recipe_images
         WHERE recipe_id = $1 AND kind = 'screenshot' ORDER BY created_at, id",
    )
    .bind(recipe_id)
    .fetch_all(pool)
    .await
}

pub async fn count_for_recipe(pool: &PgPool, recipe_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM recipe_images WHERE recipe_id = $1")
        .bind(recipe_id)
        .fetch_one(pool)
        .await
}

/// `(recipe_id, cover_image_id)`, only while the recipe is public under `slug`.
pub async fn public_cover(pool: &PgPool, slug: &str) -> Result<Option<(Uuid, Uuid)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT r.id, i.id FROM recipe_images i JOIN recipes r ON r.id = i.recipe_id
         WHERE r.public_slug = $1 AND r.is_public = true AND i.kind = 'cover'",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await
}
