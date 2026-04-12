mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn subscribe_and_unsubscribe() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    let subscription = json!({
        "endpoint": "https://push.example.com/sub1",
        "keys": { "p256dh": "abc", "auth": "def" }
    });

    // Subscribe
    let req = Request::builder()
        .method("POST")
        .uri("/api/push/subscribe")
        .header("Content-Type", "application/json")
        .header(&key, &value)
        .body(Body::from(
            serde_json::to_string(&json!({ "subscription": subscription })).unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Unsubscribe
    let req = Request::builder()
        .method("POST")
        .uri("/api/push/unsubscribe")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({ "subscription": subscription })).unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn duplicate_subscribe_is_idempotent() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    let body = serde_json::to_string(&json!({
        "subscription": {
            "endpoint": "https://push.example.com/sub2",
            "keys": { "p256dh": "abc", "auth": "def" }
        }
    }))
    .unwrap();

    for _ in 0..2 {
        let req = Request::builder()
            .method("POST")
            .uri("/api/push/subscribe")
            .header("Content-Type", "application/json")
            .header(&key, &value)
            .body(Body::from(body.clone()))
            .unwrap();

        let resp = ctx.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }
}
