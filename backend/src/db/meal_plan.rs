use sqlx::PgPool;
use time::Date;
use uuid::Uuid;

use crate::models::{CreateMealPlanRequest, MealPlanEntry, UpdateMealPlanRequest};

pub fn parse_date(s: &str) -> Result<Date, time::error::Parse> {
    Date::parse(
        s,
        &time::format_description::parse("[year]-[month]-[day]").unwrap(),
    )
}

pub async fn create(
    pool: &PgPool,
    user_id: Uuid,
    req: &CreateMealPlanRequest,
) -> Result<MealPlanEntry, sqlx::Error> {
    let date = parse_date(&req.date).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

    sqlx::query_as::<_, MealPlanEntry>(
        "INSERT INTO meal_plan_entries (user_id, date, meal_type, recipe_id, free_text, servings, status, entry_type, note)
         VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, 'confirmed'), COALESCE($8, 'logged'), $9)
         RETURNING *, (SELECT name FROM users WHERE id = $1) AS user_name",
    )
    .bind(user_id)
    .bind(date)
    .bind(&req.meal_type)
    .bind(req.recipe_id)
    .bind(&req.free_text)
    .bind(req.servings)
    .bind(&req.status)
    .bind(&req.entry_type)
    .bind(&req.note)
    .fetch_one(pool)
    .await
}

pub async fn list_by_range(
    pool: &PgPool,
    from: &str,
    to: &str,
) -> Result<Vec<MealPlanEntry>, sqlx::Error> {
    let from_date = parse_date(from).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    let to_date = parse_date(to).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

    sqlx::query_as::<_, MealPlanEntry>(
        "SELECT m.*, u.name AS user_name FROM meal_plan_entries m LEFT JOIN users u ON m.user_id = u.id WHERE m.date >= $1 AND m.date <= $2 ORDER BY m.date, m.meal_type",
    )
    .bind(from_date)
    .bind(to_date)
    .fetch_all(pool)
    .await
}

pub async fn history(pool: &PgPool, days: i64) -> Result<Vec<MealPlanEntry>, sqlx::Error> {
    sqlx::query_as::<_, MealPlanEntry>(
        "SELECT m.*, u.name AS user_name FROM meal_plan_entries m LEFT JOIN users u ON m.user_id = u.id WHERE m.date >= CURRENT_DATE - $1::integer
         ORDER BY m.date DESC, m.meal_type",
    )
    .bind(days as i32)
    .fetch_all(pool)
    .await
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    req: &UpdateMealPlanRequest,
) -> Result<Option<MealPlanEntry>, sqlx::Error> {
    let date = req
        .date
        .as_ref()
        .map(|d| parse_date(d))
        .transpose()
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

    let result = sqlx::query_as::<_, MealPlanEntry>(
        "UPDATE meal_plan_entries SET
            date = COALESCE($2, date),
            meal_type = COALESCE($3, meal_type),
            recipe_id = COALESCE($4, recipe_id),
            free_text = COALESCE($5, free_text),
            servings = COALESCE($6, servings),
            status = COALESCE($7, status),
            note = COALESCE($8, note)
         WHERE id = $1 RETURNING *, (SELECT name FROM users WHERE id = user_id) AS user_name",
    )
    .bind(id)
    .bind(date)
    .bind(&req.meal_type)
    .bind(req.recipe_id)
    .bind(&req.free_text)
    .bind(req.servings)
    .bind(&req.status)
    .bind(&req.note)
    .fetch_optional(pool)
    .await?;

    Ok(result)
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM meal_plan_entries WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
