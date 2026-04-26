mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn log_entry_created_with_valid_token() {
    let ctx = common::TestContext::new().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/log")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer test-log-token")
        .body(Body::from(
            serde_json::to_string(&json!({
                "meal_type": "dinner",
                "free_text": "švestkový koláč",
                "user_name": "Test User 1"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(status, StatusCode::CREATED, "body: {json}");
    assert_eq!(json["free_text"], "švestkový koláč");
    assert_eq!(json["meal_type"], "dinner");
    assert_eq!(json["entry_type"], "logged");
    assert_eq!(json["status"], "confirmed");
    assert_eq!(json["user_name"], "Test User 1");
}

#[tokio::test]
async fn log_entry_rejected_with_wrong_token() {
    let ctx = common::TestContext::new().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/log")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer wrong-token")
        .body(Body::from(
            serde_json::to_string(&json!({
                "meal_type": "lunch",
                "free_text": "soup",
                "user_name": "Test User 1"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn log_entry_rejected_with_unknown_user() {
    let ctx = common::TestContext::new().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/log")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer test-log-token")
        .body(Body::from(
            serde_json::to_string(&json!({
                "meal_type": "lunch",
                "free_text": "soup",
                "user_name": "nonexistent"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn log_entry_defaults_date_to_today() {
    let ctx = common::TestContext::new().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/log")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer test-log-token")
        .body(Body::from(
            serde_json::to_string(&json!({
                "meal_type": "lunch",
                "free_text": "polévka",
                "user_name": "Test User 1"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(status, StatusCode::CREATED, "body: {json}");
    // date is serialized as [year, ordinal_day] by time::Date
    assert!(
        json["date"].is_array(),
        "date should be present, got: {}",
        json["date"]
    );
}
