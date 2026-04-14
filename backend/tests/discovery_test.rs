mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

fn recipe_json() -> serde_json::Value {
    json!({
        "title": "Discovery Test Recipe",
        "description": "A recipe for discovery tests",
        "servings": 2,
        "prep_time_min": 15,
        "cook_time_min": 30,
        "source_type": "manual",
        "tags": ["test"],
        "ingredients": [
            { "name": "flour", "amount": 200.0, "unit": "g", "note": null }
        ],
        "steps": [
            { "step_order": 1, "instruction": "Mix and cook" }
        ]
    })
}

async fn create_recipe(ctx: &common::TestContext) -> serde_json::Value {
    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(serde_json::to_string(&recipe_json()).unwrap()))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn recipe_list_filters_by_status() {
    let ctx = common::TestContext::new().await;
    let created = create_recipe(&ctx).await;

    // Default status is "saved" — recipe should appear in default list (saved,tested)
    assert_eq!(created["status"], "saved");

    let (key, value) = ctx.auth_header_1();

    // Default list (no status filter = saved,tested) — should include the recipe
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes")
        .header(&key, &value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["total"], 1,
        "Recipe with status=saved should appear in default list"
    );

    // Filter by status=discovered — should NOT include the recipe
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes?status=discovered")
        .header(&key, &value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["total"], 0,
        "Recipe with status=saved should NOT appear in status=discovered list"
    );
}

#[tokio::test]
async fn recipe_status_transition_saved_to_tested() {
    let ctx = common::TestContext::new().await;
    let created = create_recipe(&ctx).await;
    let id = created["id"].as_str().unwrap();

    assert_eq!(created["status"], "saved");

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/recipes/{id}/status"))
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({ "status": "tested" })).unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "tested");
}

#[tokio::test]
async fn recipe_status_invalid_transition() {
    let ctx = common::TestContext::new().await;
    let created = create_recipe(&ctx).await;
    let id = created["id"].as_str().unwrap();

    assert_eq!(created["status"], "saved");

    // saved -> discovered is not a valid transition
    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/recipes/{id}/status"))
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({ "status": "discovered" })).unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn discover_returns_503_without_model() {
    let ctx = common::TestContext::new().await;

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/discover")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({ "count": 1 })).unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "Discovery should return 503 when embedding model is not loaded"
    );
}
