# Prometheus metrics — design

**Date:** 2026-04-27
**Topic:** Expose application metrics for scraping by an external Prometheus instance.

## Goal

Vareni currently has no observability beyond `tracing` log lines. Add a Prometheus-compatible `/metrics` endpoint that exposes:

1. **HTTP RED metrics** — request count, latency, error rate per route.
2. **Business metrics** — recipe counts, ingestion rates, meal log activity, recipe state transitions.

The user already operates Prometheus + Grafana locally; this design only covers what the application must expose. Scraping and dashboards are out of scope.

## Non-goals

- Application-level auth on `/metrics` — handled by an upstream nginx reverse proxy.
- Tracing/distributed-tracing export (only Prometheus pull metrics).
- Multi-replica deployment correctness — Vareni runs as a single instance.

## Architecture

```
External Prometheus  --(scrape via nginx auth)-->  nginx  -->  vareni-app:8080/metrics
                                                                       ^
                                                                       |
                                                       axum-prometheus Tower layer
                                                       + metrics::counter!/gauge! macros
```

- Library: `axum-prometheus` crate (wraps `metrics` + `metrics-exporter-prometheus`).
- `PrometheusMetricLayer` is added to the Axum router in `backend/src/lib.rs` next to the existing `TraceLayer`.
- The `/metrics` route is mounted on the same port (8080) — no separate metrics port.
- All custom metrics use the standard `metrics` crate macros (`counter!`, `gauge!`, `histogram!`) so they flow through the same exporter.

## HTTP RED metrics (from middleware)

Provided automatically by `axum-prometheus`:

| Metric                              | Type      | Labels                  |
|-------------------------------------|-----------|-------------------------|
| `http_requests_total`               | counter   | `method, status, endpoint` |
| `http_request_duration_seconds`     | histogram | `method, status, endpoint` |

`endpoint` is the Axum route template (e.g. `/api/recipes/{id}`), not the concrete path. UUIDs and slugs do not cause cardinality explosion.

## Business metrics

| Metric                          | Type     | Labels                          | Source                                       |
|---------------------------------|----------|---------------------------------|----------------------------------------------|
| `recipes_total`                 | gauge    | —                               | Background task: `SELECT COUNT(*) FROM recipes` every `METRICS_GAUGE_REFRESH_SECS` (default 60s) |
| `recipe_ingests_total`          | counter  | `source` ∈ {url, manual}, `status` ∈ {ok, error} | Incremented in `routes/ingest.rs` and `routes/recipes.rs::create` |
| `meal_log_entries_total`        | counter  | `meal_type`, `source` ∈ {spa, ha_api} | Incremented when a meal entry is created with `entry_type = "logged"` (i.e. an actually-eaten meal, not a planned one). `routes/log.rs` always counts as `source = ha_api`; `routes/plan.rs::create` counts as `source = spa` only when `entry_type = "logged"`. |
| `recipe_status_changes_total`   | counter  | `from_status, to_status`        | Incremented in `routes/recipes.rs::update_status` |

### Cardinality safety

- `meal_type` must be deserialized as a Rust enum (not a free-form string), so an arbitrary value sent via the HA API token cannot inflate label cardinality. The enum is the authoritative set of allowed values.
- `from_status`/`to_status` are already enum-typed in the existing recipe model.
- `source` labels are static literal strings set at the call site.

### Gauge correctness

`recipes_total` is **not** maintained by per-handler increment/decrement. A background tokio task runs a `COUNT(*)` query every 60s and writes the result to the gauge via `metrics::gauge!(...).set(value)`. This avoids drift from cascade deletes, rolled-back transactions, raw DB writes, or process crashes between DB commit and gauge update.

The interval is configurable via `METRICS_GAUGE_REFRESH_SECS`.

## New module: `backend/src/metrics.rs`

Single new module. Responsibilities:

1. Build the `PrometheusMetricLayer` and the `/metrics` handler. Returned as a tuple `(layer, handle)` consumed by `lib.rs::create_router`.
2. Spawn the background gauge-refresh task (takes a `PgPool` clone). The task is `tokio::spawn`ed at startup from `main.rs`.
3. Define the `MealType` enum used for deserialization in `models.rs` (or extend the existing one if it already exists).

`lib.rs` only changes by:
- Adding `pub mod metrics;`
- Wiring the layer into the router.
- Adding the `/metrics` route.

`main.rs` only changes by spawning the gauge-refresh task after `PgPool::connect` succeeds.

## Test setup

The `metrics` crate panics if no recorder is installed when a macro fires. Integration tests exercise the handlers but don't go through `create_router` with the metrics layer attached, so they have no recorder.

Fix: in `backend/tests/common/mod.rs`, install a `metrics::NoopRecorder` (or equivalent from the `metrics` crate) before any test code runs. This is one line in the test bootstrap.

## Configuration

| Env var                       | Default | Purpose                                |
|-------------------------------|---------|----------------------------------------|
| `METRICS_GAUGE_REFRESH_SECS`  | `60`    | Cadence of `recipes_total` DB query.   |

No env var to disable metrics — they are always on. Cost is negligible and there's no reason to turn them off.

## Failure modes

| Scenario                          | Behavior                                                            |
|-----------------------------------|---------------------------------------------------------------------|
| Prometheus is down                | Metrics buffer in process memory, no impact on app.                 |
| `/metrics` handler panics         | Axum returns 500, Prometheus records a scrape gap. No state damage. |
| Gauge refresh DB query fails      | Log error, keep last value, retry next interval.                    |
| App restarts                      | Counters reset to 0 (standard Prometheus behavior — `rate()` handles it). Gauge re-populated within one refresh interval. |

## Out of scope (explicit fix-later list)

- **`is_public` / `auth` label on HTTP RED** — useful to distinguish public recipe traffic from authenticated traffic in Grafana, but adds complexity. Defer until there's a concrete need.
- **DB query duration histograms** — could be useful, but `sqlx` doesn't have a built-in metrics hook and wrapping every query is invasive. Defer.
- **Per-user activity metrics** — privacy-sensitive, no clear use case yet.

## Files touched (estimated)

- `backend/Cargo.toml` — add `axum-prometheus` and `metrics` (transitive).
- `backend/src/metrics.rs` — new module.
- `backend/src/lib.rs` — wire layer, mount `/metrics` route.
- `backend/src/main.rs` — spawn gauge-refresh task.
- `backend/src/models.rs` — `MealType` enum (if not already present).
- `backend/src/routes/ingest.rs` — increment `recipe_ingests_total`.
- `backend/src/routes/recipes.rs` — increment ingest counter on create, status-change counter on update_status.
- `backend/src/routes/log.rs` — increment `meal_log_entries_total{source=ha_api}` (every call).
- `backend/src/routes/plan.rs::create` — increment `meal_log_entries_total{source=spa}` only when `entry_type = "logged"`.
- `backend/tests/common/mod.rs` — install noop recorder.
