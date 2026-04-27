use std::sync::OnceLock;
use std::time::Duration;

use axum_prometheus::PrometheusMetricLayerBuilder;
use axum_prometheus::metrics_exporter_prometheus::PrometheusHandle;
use sqlx::PgPool;

pub const RECIPES_TOTAL: &str = "recipes_total";
pub const RECIPE_INGESTS_TOTAL: &str = "recipe_ingests_total";
pub const MEAL_LOG_ENTRIES_TOTAL: &str = "meal_log_entries_total";
pub const RECIPE_STATUS_CHANGES_TOTAL: &str = "recipe_status_changes_total";

// The Prometheus recorder is process-global and can only be installed once.
// This memoization makes `setup()` callable many times (e.g. across tests
// that each build their own router) without panicking.
static SETUP: OnceLock<(
    axum_prometheus::PrometheusMetricLayer<'static>,
    PrometheusHandle,
)> = OnceLock::new();

pub fn setup() -> (
    axum_prometheus::PrometheusMetricLayer<'static>,
    PrometheusHandle,
) {
    SETUP
        .get_or_init(|| {
            PrometheusMetricLayerBuilder::new()
                .with_default_metrics()
                .build_pair()
        })
        .clone()
}

pub fn spawn_gauge_refresh(pool: PgPool, interval_secs: u64) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;
            match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM recipes")
                .fetch_one(&pool)
                .await
            {
                Ok(count) => {
                    metrics::gauge!(RECIPES_TOTAL).set(count as f64);
                }
                Err(e) => {
                    tracing::warn!("metrics: failed to refresh recipes_total gauge: {e}");
                }
            }
        }
    });
}
