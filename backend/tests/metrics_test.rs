mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn metrics_endpoint_returns_prometheus_text() {
    let ctx = common::TestContext::new().await;

    // Generate at least one completed request so the HTTP RED counter is present.
    let warmup = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .body(Body::empty())
        .unwrap();
    let _ = ctx.router.clone().oneshot(warmup).await.unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();

    assert!(
        text.contains("axum_http_requests_total"),
        "metrics output missing http_requests counter:\n{text}"
    );
}

#[tokio::test]
async fn meal_log_entry_increments_counter() {
    let ctx = common::TestContext::new().await;

    // Hit the log endpoint to bump meal_log_entries_total{source=ha_api}
    let req = Request::builder()
        .method("POST")
        .uri("/api/log")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer test-log-token")
        .body(Body::from(
            serde_json::to_string(&json!({
                "meal_type": "dinner",
                "free_text": "test meal",
                "user_name": "Test User 1"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Scrape /metrics and assert the counter is present with the expected labels
    let req = Request::builder()
        .method("GET")
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();

    assert!(
        text.contains("meal_log_entries_total")
            && text.contains("source=\"ha_api\"")
            && text.contains("meal_type=\"dinner\""),
        "expected meal_log_entries_total{{source=ha_api,meal_type=dinner}} in:\n{text}"
    );
}
