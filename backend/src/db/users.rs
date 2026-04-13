use sqlx::PgPool;
use uuid::Uuid;

use crate::models::User;

pub async fn list_all(pool: &PgPool) -> Result<Vec<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "SELECT id, name, email, password_hash, created_at FROM users ORDER BY name",
    )
    .fetch_all(pool)
    .await
}

pub async fn find_by_name(pool: &PgPool, name: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "SELECT id, name, email, password_hash, created_at FROM users WHERE name = $1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "SELECT id, name, email, password_hash, created_at FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn get_dietary_restrictions(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT restriction FROM user_dietary_restrictions WHERE user_id = $1 ORDER BY restriction",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_all_dietary_restrictions(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT restriction FROM user_dietary_restrictions ORDER BY restriction",
    )
    .fetch_all(pool)
    .await
}

pub async fn add_dietary_restriction(
    pool: &PgPool,
    user_id: Uuid,
    restriction: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO user_dietary_restrictions (user_id, restriction) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(restriction)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_dietary_restriction(
    pool: &PgPool,
    user_id: Uuid,
    restriction: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM user_dietary_restrictions WHERE user_id = $1 AND restriction = $2",
    )
    .bind(user_id)
    .bind(restriction)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
