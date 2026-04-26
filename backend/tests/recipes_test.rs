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
        "source_type": "manual",
        "tags": ["pasta", "quick", "Italian"],
        "sections": [
            {
                "label": null,
                "description": null,
                "prep_time_min": 10,
                "cook_time_min": 20,
                "sort_order": 0,
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
            }
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
    assert_eq!(json["sections"].as_array().unwrap().len(), 1);
    assert_eq!(
        json["sections"][0]["ingredients"].as_array().unwrap().len(),
        3
    );
    assert_eq!(json["sections"][0]["steps"].as_array().unwrap().len(), 3);
    assert_eq!(json["sections"][0]["label"], serde_json::Value::Null);
    assert_eq!(json["prep_time_min"], 10); // derived from single section
    assert_eq!(json["cook_time_min"], 20);
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
    assert_eq!(json["sections"].as_array().unwrap().len(), 1);
    assert_eq!(
        json["sections"][0]["ingredients"].as_array().unwrap().len(),
        3
    );
    assert_eq!(json["sections"][0]["steps"].as_array().unwrap().len(), 3);
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
                "source_type": "manual",
                "sections": [
                    {
                        "label": null,
                        "sort_order": 0,
                        "ingredients": [
                            { "name": "flour", "amount": 200.0, "unit": "g", "note": "for dough" },
                            { "name": "flour", "amount": 30.0, "unit": "g", "note": "for dusting" }
                        ],
                        "steps": [
                            { "step_order": 1, "instruction": "Make dough" }
                        ]
                    }
                ]
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Two ingredient entries, same ingredient name, in the first section
    assert_eq!(
        json["sections"][0]["ingredients"].as_array().unwrap().len(),
        2
    );
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

#[tokio::test]
async fn create_recipe_with_multiple_sections() {
    let ctx = common::TestContext::new().await;
    let payload = json!({
        "title": "Plum Cake",
        "servings": 8,
        "source_type": "manual",
        "tags": ["cake"],
        "sections": [
            {
                "label": "Těsto",
                "prep_time_min": 15,
                "cook_time_min": 40,
                "sort_order": 0,
                "ingredients": [
                    { "name": "flour", "amount": 250.0, "unit": "g", "note": null },
                    { "name": "sugar", "amount": 150.0, "unit": "g", "note": null }
                ],
                "steps": [
                    { "step_order": 1, "instruction": "Mix flour and sugar" }
                ]
            },
            {
                "label": "Náplň",
                "prep_time_min": 5,
                "cook_time_min": null,
                "sort_order": 1,
                "ingredients": [
                    { "name": "tvaroh", "amount": 500.0, "unit": "g", "note": null }
                ],
                "steps": [
                    { "step_order": 1, "instruction": "Mash tvaroh" }
                ]
            }
        ]
    });
    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["sections"].as_array().unwrap().len(), 2);
    assert_eq!(json["sections"][0]["label"], "Těsto");
    assert_eq!(json["sections"][1]["label"], "Náplň");
    assert_eq!(
        json["sections"][0]["ingredients"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        json["sections"][1]["ingredients"].as_array().unwrap().len(),
        1
    );
    assert_eq!(json["prep_time_min"], 20); // 15 + 5
    assert_eq!(json["cook_time_min"], 40); // 40 + null
}

#[tokio::test]
async fn create_recipe_rejects_empty_sections() {
    let ctx = common::TestContext::new().await;
    let payload = json!({
        "title": "Bad",
        "source_type": "manual",
        "sections": []
    });
    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_recipe_rejects_foreign_section_id() {
    let ctx = common::TestContext::new().await;
    let recipe_a = create_recipe(&ctx).await;
    let recipe_b = create_recipe(&ctx).await;
    let foreign_section_id = recipe_b["sections"][0]["id"].as_str().unwrap();
    let recipe_a_id = recipe_a["id"].as_str().unwrap();

    let payload = json!({
        "sections": [
            {
                "id": foreign_section_id,
                "label": "Pwned",
                "sort_order": 0,
                "ingredients": [],
                "steps": []
            }
        ]
    });
    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/recipes/{}", recipe_a_id))
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_recipe_deletes_removed_section() {
    let ctx = common::TestContext::new().await;
    // create a 2-section recipe
    let payload = json!({
        "title": "Two Section",
        "source_type": "manual",
        "tags": [],
        "sections": [
            { "label": "A", "sort_order": 0, "ingredients": [{ "name": "x", "amount": 1.0, "unit": "g", "note": null }], "steps": [] },
            { "label": "B", "sort_order": 1, "ingredients": [{ "name": "y", "amount": 1.0, "unit": "g", "note": null }], "steps": [] }
        ]
    });
    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .header(key.clone(), value.clone())
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let recipe_id = created["id"].as_str().unwrap();
    let section_a_id = created["sections"][0]["id"].as_str().unwrap();

    // PUT with only section A
    let update = json!({
        "sections": [
            { "id": section_a_id, "label": "A", "sort_order": 0, "ingredients": [{ "name": "x", "amount": 1.0, "unit": "g", "note": null }], "steps": [] }
        ]
    });
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/recipes/{}", recipe_id))
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(serde_json::to_string(&update).unwrap()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["sections"].as_array().unwrap().len(), 1);
    assert_eq!(json["sections"][0]["label"], "A");
}

#[tokio::test]
async fn update_recipe_reorders_sections() {
    let ctx = common::TestContext::new().await;
    let payload = json!({
        "title": "Reorder",
        "source_type": "manual",
        "tags": [],
        "sections": [
            { "label": "First",  "sort_order": 0, "ingredients": [], "steps": [] },
            { "label": "Second", "sort_order": 1, "ingredients": [], "steps": [] },
            { "label": "Third",  "sort_order": 2, "ingredients": [], "steps": [] }
        ]
    });
    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .header(key.clone(), value.clone())
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let recipe_id = created["id"].as_str().unwrap();
    let s0 = created["sections"][0]["id"].as_str().unwrap();
    let s1 = created["sections"][1]["id"].as_str().unwrap();
    let s2 = created["sections"][2]["id"].as_str().unwrap();

    let update = json!({
        "sections": [
            { "id": s2, "label": "Third",  "sort_order": 0, "ingredients": [], "steps": [] },
            { "id": s1, "label": "Second", "sort_order": 1, "ingredients": [], "steps": [] },
            { "id": s0, "label": "First",  "sort_order": 2, "ingredients": [], "steps": [] }
        ]
    });
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/recipes/{}", recipe_id))
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(serde_json::to_string(&update).unwrap()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let labels: Vec<&str> = json["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, vec!["Third", "Second", "First"]);
}

#[tokio::test]
async fn update_status_returns_derived_times() {
    let ctx = common::TestContext::new().await;
    let json_resp = create_recipe(&ctx).await;
    let recipe_id = json_resp["id"].as_str().unwrap();

    let (key, value) = ctx.auth_header_1();
    let body = json!({ "status": "tested" });
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/recipes/{}/status", recipe_id))
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // The fixture from recipe_json() puts prep=10 cook=20 in section[0].
    assert_eq!(json["prep_time_min"], 10);
    assert_eq!(json["cook_time_min"], 20);
}

#[tokio::test]
async fn update_recipe_rejects_duplicate_section_id() {
    let ctx = common::TestContext::new().await;
    let json_resp = create_recipe(&ctx).await;
    let recipe_id = json_resp["id"].as_str().unwrap();
    let section_id = json_resp["sections"][0]["id"].as_str().unwrap();

    let (key, value) = ctx.auth_header_1();
    let payload = json!({
        "sections": [
            { "id": section_id, "label": "A", "sort_order": 0, "ingredients": [], "steps": [] },
            { "id": section_id, "label": "B", "sort_order": 1, "ingredients": [], "steps": [] }
        ]
    });
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/recipes/{}", recipe_id))
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_recipe_renumbers_step_order() {
    let ctx = common::TestContext::new().await;
    let payload = json!({
        "title": "Bad Step Order",
        "source_type": "manual",
        "tags": [],
        "sections": [
            {
                "label": null, "sort_order": 0,
                "ingredients": [],
                "steps": [
                    { "step_order": 1, "instruction": "first" },
                    { "step_order": 1, "instruction": "second" },
                    { "step_order": 1, "instruction": "third" }
                ]
            }
        ]
    });
    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let steps = json["sections"][0]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 3);
    let orders: Vec<i64> = steps
        .iter()
        .map(|s| s["step_order"].as_i64().unwrap())
        .collect();
    assert_eq!(orders, vec![1, 2, 3]);
}
