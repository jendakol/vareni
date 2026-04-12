mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

fn recipe_json() -> serde_json::Value {
    json!({
        "title": "Test Pasta",
        "description": "A simple pasta dish",
        "servings": 4,
        "prep_time_min": 10,
        "cook_time_min": 20,
        "source_type": "manual",
        "tags": ["pasta", "quick", "Italian"],
        "ingredients": [
            { "name": "pasta", "amount": 400.0, "unit": "g", "note": null },
            { "name": "olive oil", "amount": 2.0, "unit": "tbsp", "note": null },
            { "name": "garlic", "amount": 3.0, "unit": "cloves", "note": "minced" }
        ],
        "steps": [
            { "step_order": 1, "instruction": "Boil water and cook pasta" },
            { "step_order": 2, "instruction": "Sauté garlic in olive oil" },
            { "step_order": 3, "instruction": "Combine and serve" }
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
async fn create_recipe_returns_full_detail() {
    let ctx = common::TestContext::new().await;
    let json = create_recipe(&ctx).await;

    assert_eq!(json["title"], "Test Pasta");
    assert_eq!(json["ingredients"].as_array().unwrap().len(), 3);
    assert_eq!(json["steps"].as_array().unwrap().len(), 3);
    assert_eq!(json["tags"].as_array().unwrap().len(), 3);
    assert!(json["id"].is_string());
}

#[tokio::test]
async fn list_recipes_paginated() {
    let ctx = common::TestContext::new().await;
    create_recipe(&ctx).await;
    create_recipe(&ctx).await;

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes?page=1&per_page=10")
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 2);
    assert_eq!(json["items"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn list_recipes_search() {
    let ctx = common::TestContext::new().await;
    create_recipe(&ctx).await; // "Test Pasta"

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes?q=Pasta")
        .header(&key, &value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 1);

    // Search for something that doesn't exist
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes?q=sushi")
        .header(&key, &value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 0);
}

#[tokio::test]
async fn list_recipes_tag_filter() {
    let ctx = common::TestContext::new().await;
    create_recipe(&ctx).await; // tags: pasta, quick, Italian

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes?tag=pasta")
        .header(&key, &value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 1);

    // Non-matching tag
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes?tag=dessert")
        .header(&key, &value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 0);
}

#[tokio::test]
async fn get_recipe_detail() {
    let ctx = common::TestContext::new().await;
    let created = create_recipe(&ctx).await;
    let id = created["id"].as_str().unwrap();

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/recipes/{id}"))
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["title"], "Test Pasta");
    assert_eq!(json["ingredients"].as_array().unwrap().len(), 3);
    assert_eq!(json["steps"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn update_recipe() {
    let ctx = common::TestContext::new().await;
    let created = create_recipe(&ctx).await;
    let id = created["id"].as_str().unwrap();

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/recipes/{id}"))
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "title": "Updated Pasta",
                "tags": ["pasta", "updated"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["title"], "Updated Pasta");
    assert_eq!(json["tags"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn delete_recipe() {
    let ctx = common::TestContext::new().await;
    let created = create_recipe(&ctx).await;
    let id = created["id"].as_str().unwrap();

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/recipes/{id}"))
        .header(&key, &value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Confirm it's gone
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/recipes/{id}"))
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn both_users_see_all_recipes() {
    let ctx = common::TestContext::new().await;
    create_recipe(&ctx).await; // created by user1

    // user2 can see it
    let (key, value) = ctx.auth_header_2();
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes")
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 1);
}

#[tokio::test]
async fn duplicate_ingredient_in_recipe() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "title": "Flour Test",
                "ingredients": [
                    { "name": "flour", "amount": 200.0, "unit": "g", "note": "for dough" },
                    { "name": "flour", "amount": 30.0, "unit": "g", "note": "for dusting" }
                ],
                "steps": [
                    { "step_order": 1, "instruction": "Make dough" }
                ]
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Two ingredient entries, same ingredient name
    assert_eq!(json["ingredients"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn recipe_crud_without_auth_is_unauthorized() {
    let ctx = common::TestContext::new().await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes")
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
