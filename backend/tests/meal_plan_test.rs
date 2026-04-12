mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

async fn create_recipe_for_plan(ctx: &common::TestContext) -> String {
    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "title": "Plan Recipe",
                "ingredients": [{ "name": "salt", "amount": 1.0, "unit": "tsp" }],
                "steps": [{ "step_order": 1, "instruction": "Season" }]
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "Failed to create recipe: {json}"
    );
    json["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn create_meal_plan_with_recipe() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe_for_plan(&ctx).await;

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/plan")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "date": "2026-04-15",
                "meal_type": "dinner",
                "recipe_id": recipe_id,
                "servings": 2
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_meal_plan_with_free_text() {
    let ctx = common::TestContext::new().await;

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/plan")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "date": "2026-04-15",
                "meal_type": "lunch",
                "free_text": "Leftover pizza"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_meal_plan_without_recipe_or_text_fails() {
    let ctx = common::TestContext::new().await;

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/plan")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "date": "2026-04-15",
                "meal_type": "lunch"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    // Should fail due to CHECK constraint
    assert!(resp.status().is_server_error() || resp.status().is_client_error());
}

#[tokio::test]
async fn list_by_date_range() {
    let ctx = common::TestContext::new().await;

    let (key, value) = ctx.auth_header_1();
    // Create two entries on different dates
    for date in ["2026-04-15", "2026-04-20"] {
        let req = Request::builder()
            .method("POST")
            .uri("/api/plan")
            .header("Content-Type", "application/json")
            .header(&key, &value)
            .body(Body::from(
                serde_json::to_string(&json!({
                    "date": date,
                    "meal_type": "dinner",
                    "free_text": "Something"
                }))
                .unwrap(),
            ))
            .unwrap();
        ctx.router.clone().oneshot(req).await.unwrap();
    }

    // Query range that includes only the first
    let req = Request::builder()
        .method("GET")
        .uri("/api/plan?from=2026-04-14&to=2026-04-16")
        .header(&key, &value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn update_meal_plan_status() {
    let ctx = common::TestContext::new().await;

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/plan")
        .header("Content-Type", "application/json")
        .header(&key, &value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "date": "2026-04-15",
                "meal_type": "dinner",
                "free_text": "Soup"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap();

    // Update status to cooked
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/plan/{id}"))
        .header("Content-Type", "application/json")
        .header(&key, &value)
        .body(Body::from(
            serde_json::to_string(&json!({ "status": "cooked" })).unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "cooked");
}

#[tokio::test]
async fn delete_meal_plan_entry() {
    let ctx = common::TestContext::new().await;

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/plan")
        .header("Content-Type", "application/json")
        .header(&key, &value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "date": "2026-04-15",
                "meal_type": "dinner",
                "free_text": "Delete me"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/plan/{id}"))
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}
