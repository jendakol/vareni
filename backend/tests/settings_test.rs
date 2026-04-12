mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn add_and_remove_dietary_restriction() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    // Add restriction
    let req = Request::builder()
        .method("POST")
        .uri("/api/settings/restrictions")
        .header("Content-Type", "application/json")
        .header(&key, &value)
        .body(Body::from(
            serde_json::to_string(&json!({ "restriction": "vegetarian" })).unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Verify via /me
    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .header(&key, &value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["dietary_restrictions"]
            .as_array()
            .unwrap()
            .contains(&json!("vegetarian"))
    );

    // Remove restriction
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/settings/restrictions")
        .header("Content-Type", "application/json")
        .header(&key, &value)
        .body(Body::from(
            serde_json::to_string(&json!({ "restriction": "vegetarian" })).unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify it's gone
    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["dietary_restrictions"].as_array().unwrap().is_empty());
}
