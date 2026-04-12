use sqlx::PgPool;
use time::OffsetDateTime;
use tracing;

pub fn start_notifier(pool: PgPool, notify_hour: u32) {
    tokio::spawn(async move {
        loop {
            let now = OffsetDateTime::now_utc();
            let current_hour = now.hour() as u32;

            if current_hour == notify_hour {
                if let Err(e) = check_and_notify(&pool).await {
                    tracing::error!("Push notification error: {e}");
                }
                // Sleep until next day (roughly)
                tokio::time::sleep(std::time::Duration::from_secs(23 * 3600)).await;
            } else {
                // Sleep 30 minutes and check again
                tokio::time::sleep(std::time::Duration::from_secs(1800)).await;
            }
        }
    });
}

async fn check_and_notify(pool: &PgPool) -> anyhow::Result<()> {
    // Find users with push subscriptions who haven't logged dinner today
    let rows = sqlx::query_as::<_, (uuid::Uuid, serde_json::Value)>(
        "SELECT ps.user_id, ps.subscription FROM push_subscriptions ps
         WHERE ps.user_id NOT IN (
             SELECT user_id FROM meal_plan_entries
             WHERE date = CURRENT_DATE AND meal_type = 'dinner'
         )",
    )
    .fetch_all(pool)
    .await?;

    for (_user_id, _subscription) in rows {
        // TODO: send web push notification via web-push crate
        // This requires VAPID keys which will be configured in production
        tracing::info!("Would send dinner reminder push notification");
    }

    Ok(())
}
