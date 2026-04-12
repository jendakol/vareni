use sqlx::PgPool;
use uuid::Uuid;

pub async fn subscribe(
    pool: &PgPool,
    user_id: Uuid,
    subscription: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    // Upsert: delete existing + insert (simple for 2-user app)
    sqlx::query(
        "INSERT INTO push_subscriptions (user_id, subscription) VALUES ($1, $2) ON CONFLICT (user_id, subscription) DO NOTHING",
    )
    .bind(user_id)
    .bind(subscription)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn unsubscribe(
    pool: &PgPool,
    user_id: Uuid,
    subscription: &serde_json::Value,
) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("DELETE FROM push_subscriptions WHERE user_id = $1 AND subscription = $2")
            .bind(user_id)
            .bind(subscription)
            .execute(pool)
            .await?;
    Ok(result.rows_affected() > 0)
}
