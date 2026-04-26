mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

async fn create_and_share(ctx: &common::TestContext) -> (String, String) {
    // Create a recipe
    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .header(&key, &value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "title": "Shared Recipe",
                "source_type": "manual",
                "sections": [
                    {
                        "label": null,
                        "sort_order": 0,
                        "ingredients": [{ "name": "salt", "amount": 1.0, "unit": "tsp" }],
                        "steps": [{ "step_order": 1, "instruction": "Add salt" }]
                    }
                ]
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let recipe: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let recipe_id = recipe["id"].as_str().unwrap().to_string();

    // Share it
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/recipes/{recipe_id}/share"))
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let share: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let slug = share["slug"].as_str().unwrap().to_string();

    (recipe_id, slug)
}

#[tokio::test]
async fn share_and_access_public_recipe() {
    let ctx = common::TestContext::new().await;
    let (_recipe_id, slug) = create_and_share(&ctx).await;

    // Access without auth
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/public/recipes/{slug}"))
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["title"], "Shared Recipe");
}

#[tokio::test]
async fn public_recipe_nonexistent_slug() {
    let ctx = common::TestContext::new().await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/public/recipes/nonexistent-slug")
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unshare_revokes_access() {
    let ctx = common::TestContext::new().await;
    let (recipe_id, slug) = create_and_share(&ctx).await;

    // Unshare
    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/recipes/{recipe_id}/share"))
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Public access should now fail
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/public/recipes/{slug}"))
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
