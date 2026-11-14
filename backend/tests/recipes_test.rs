mod common;

use common::WithAuth;

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
    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .with_auth(&auth)
        .body(Body::from(serde_json::to_string(&recipe_json()).unwrap()))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

/// Create a recipe with a given title and prep time. Returns its id.
async fn create_named(ctx: &common::TestContext, title: &str, prep_time_min: i64) -> String {
    let auth = ctx.auth_1();
    let mut body = recipe_json();
    body["title"] = json!(title);
    body["sections"][0]["prep_time_min"] = json!(prep_time_min);
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .with_auth(&auth)
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["id"].as_str().unwrap().to_string()
}

/// PATCH a recipe's status. Only transitions allowed by `is_valid_transition` succeed.
async fn set_status(ctx: &common::TestContext, id: &str, status: &str) {
    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/recipes/{id}/status"))
        .header("Content-Type", "application/json")
        .with_auth(&auth)
        .body(Body::from(json!({ "status": status }).to_string()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// GET /api/recipes with a raw query string; returns the titles in response order.
async fn list_titles(ctx: &common::TestContext, query: &str) -> Vec<String> {
    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/recipes?{query}"))
        .with_auth(&auth)
        .body(Body::empty())
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "query: {query}");
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn search_combined_with_each_sort_returns_ok() {
    let ctx = common::TestContext::new().await;
    create_named(&ctx, "Pasta Verde", 10).await;

    // Before the fix, SELECT DISTINCT + an ORDER BY expression outside the select
    // list makes Postgres reject these two outright.
    for sort in ["recent", "least_cooked", "prep_time"] {
        let titles = list_titles(&ctx, &format!("q=pasta&sort={sort}")).await;
        assert!(
            titles.iter().any(|t| t == "Pasta Verde"),
            "sort={sort} returned {titles:?}"
        );
    }
}

#[tokio::test]
async fn tested_recipes_sort_before_saved_in_every_sort_mode() {
    let ctx = common::TestContext::new().await;
    let slow = create_named(&ctx, "Slow Tested", 90).await;
    create_named(&ctx, "Fast Saved", 1).await;
    set_status(&ctx, &slow, "tested").await;

    for sort in ["recent", "least_cooked", "prep_time"] {
        let titles = list_titles(&ctx, &format!("sort={sort}")).await;
        assert_eq!(
            titles.first().map(String::as_str),
            Some("Slow Tested"),
            "sort={sort} returned {titles:?}"
        );
    }
}

#[tokio::test]
async fn tag_filter_combined_with_each_sort_returns_ok() {
    let ctx = common::TestContext::new().await;
    // create_named keeps recipe_json()'s default tags: pasta, quick, Italian.
    create_named(&ctx, "Tagged Pasta", 10).await;

    // Before the fix, the tag branch shared the same join/order interpolation as
    // the q branch, so a malformed hoist here would produce invalid SQL.
    for sort in ["recent", "least_cooked", "prep_time"] {
        let titles = list_titles(&ctx, &format!("tag=pasta&sort={sort}")).await;
        assert!(
            titles.iter().any(|t| t == "Tagged Pasta"),
            "sort={sort} returned {titles:?}"
        );
    }
}

#[tokio::test]
async fn paging_with_identical_timestamps_yields_each_recipe_once() {
    let ctx = common::TestContext::new().await;
    let mut created: Vec<(String, String)> = Vec::new();
    for i in 0..5 {
        let title = format!("Recipe {i}");
        let id = create_named(&ctx, &title, 10).await;
        created.push((id, title));
    }
    // Force a full tie on the primary sort key.
    sqlx::query("UPDATE recipes SET created_at = now()")
        .execute(&ctx.pool)
        .await
        .unwrap();

    let mut seen = Vec::new();
    for page in 1..=3 {
        seen.extend(list_titles(&ctx, &format!("page={page}&per_page=2")).await);
    }

    // Do NOT sort `seen` — with status and created_at tied, the post-fix order is
    // pure r.id DESC; pre-fix it is whatever the scan happens to return, which a
    // set comparison can't tell apart from the correct order.
    let mut expected = created;
    expected.sort_by(|a, b| b.0.cmp(&a.0)); // id DESC
    let expected_titles: Vec<String> = expected.into_iter().map(|(_, t)| t).collect();
    assert_eq!(
        seen, expected_titles,
        "page order did not follow the r.id DESC tiebreaker"
    );
}

#[tokio::test]
async fn recent_sort_follows_creation_not_last_edit() {
    let ctx = common::TestContext::new().await;
    create_named(&ctx, "Older", 10).await;
    create_named(&ctx, "Newer", 10).await;
    // Editing the older recipe must not lift it above the newer one.
    sqlx::query("UPDATE recipes SET updated_at = now() + interval '1 hour' WHERE title = 'Older'")
        .execute(&ctx.pool)
        .await
        .unwrap();

    assert_eq!(list_titles(&ctx, "sort=recent").await, ["Newer", "Older"]);
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

    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes?page=1&per_page=10")
        .with_auth(&auth)
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

    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes?q=Pasta")
        .with_auth(&auth)
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
        .with_auth(&auth)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 0);
}

#[tokio::test]
async fn search_ignores_diacritics_and_case() {
    let ctx = common::TestContext::new().await;
    let auth = ctx.auth_1();

    // Create a recipe with Czech diacritics in the title
    let payload = json!({
        "title": "Palačinky se šlehačkou",
        "source_type": "manual",
        "sections": [
            { "label": null, "sort_order": 0, "ingredients": [], "steps": [
                { "step_order": 1, "instruction": "Udělej palačinky" }
            ]}
        ]
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .with_auth(&auth)
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Search without diacritics — must still find it
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes?q=palacinky")
        .with_auth(&auth)
        .body(Body::empty())
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["total"], 1,
        "search without diacritics should find recipe"
    );

    // Search uppercase without diacritics — must still find it
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes?q=SLEHACKOU")
        .with_auth(&auth)
        .body(Body::empty())
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["total"], 1,
        "uppercase search without diacritics should find recipe"
    );
}

#[tokio::test]
async fn list_recipes_tag_filter() {
    let ctx = common::TestContext::new().await;
    create_recipe(&ctx).await; // tags: pasta, quick, Italian

    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes?tag=pasta")
        .with_auth(&auth)
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
        .with_auth(&auth)
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

    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/recipes/{id}"))
        .with_auth(&auth)
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

    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/recipes/{id}"))
        .header("Content-Type", "application/json")
        .with_auth(&auth)
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

    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/recipes/{id}"))
        .with_auth(&auth)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Confirm it's gone
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/recipes/{id}"))
        .with_auth(&auth)
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
    let auth = ctx.auth_2();
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes")
        .with_auth(&auth)
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
    let auth = ctx.auth_1();

    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .with_auth(&auth)
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
    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .with_auth(&auth)
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
    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .with_auth(&auth)
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
    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/recipes/{}", recipe_a_id))
        .header("Content-Type", "application/json")
        .with_auth(&auth)
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
    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .with_auth(&auth)
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
        .with_auth(&auth)
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
    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .with_auth(&auth)
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
        .with_auth(&auth)
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

    let auth = ctx.auth_1();
    let body = json!({ "status": "tested" });
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/recipes/{}/status", recipe_id))
        .header("Content-Type", "application/json")
        .with_auth(&auth)
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

    let auth = ctx.auth_1();
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
        .with_auth(&auth)
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
    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .with_auth(&auth)
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

/// PUT a partial update and return the resulting recipe detail.
async fn put_recipe(
    ctx: &common::TestContext,
    id: &str,
    body: serde_json::Value,
) -> serde_json::Value {
    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/recipes/{id}"))
        .header("Content-Type", "application/json")
        .with_auth(&auth)
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn text_fields_keep_on_absent_set_on_value_clear_on_empty_string() {
    let ctx = common::TestContext::new().await;
    let id = create_named(&ctx, "Linky", 10).await;

    // Set all three.
    let r = put_recipe(
        &ctx,
        &id,
        json!({
            "description": "Popis receptu",
            "emoji": "🍝",
            "source_url": "https://apetitonline.cz/recept",
        }),
    )
    .await;
    assert_eq!(r["description"], "Popis receptu");
    assert_eq!(r["emoji"], "🍝");
    assert_eq!(r["source_url"], "https://apetitonline.cz/recept");

    // Absent -> keep. This is the case that protects a discovered recipe's URL
    // during a section-only edit.
    let r = put_recipe(&ctx, &id, json!({ "title": "Linky 2" })).await;
    assert_eq!(r["title"], "Linky 2");
    assert_eq!(r["description"], "Popis receptu");
    assert_eq!(r["emoji"], "🍝");
    assert_eq!(r["source_url"], "https://apetitonline.cz/recept");

    // Empty string -> clear.
    let r = put_recipe(
        &ctx,
        &id,
        json!({ "description": "", "emoji": "", "source_url": "" }),
    )
    .await;
    assert_eq!(r["description"], serde_json::Value::Null);
    assert_eq!(r["emoji"], serde_json::Value::Null);
    assert_eq!(r["source_url"], serde_json::Value::Null);
}

#[tokio::test]
async fn mixed_keep_clear_set_in_one_request_does_not_cross_fields() {
    let ctx = common::TestContext::new().await;
    let id = create_named(&ctx, "Fusilli", 10).await;

    // Seed all three with distinct, recognisable values.
    let r = put_recipe(
        &ctx,
        &id,
        json!({
            "description": "Desc-Alpha",
            "emoji": "🥐",
            "source_url": "https://one.example/alpha",
        }),
    )
    .await;
    assert_eq!(r["description"], "Desc-Alpha");
    assert_eq!(r["emoji"], "🥐");
    assert_eq!(r["source_url"], "https://one.example/alpha");

    // Round 1: clear description, keep emoji (omitted), set source_url to a new value.
    let r = put_recipe(
        &ctx,
        &id,
        json!({ "description": "", "source_url": "https://two.example/beta" }),
    )
    .await;
    assert_eq!(r["description"], serde_json::Value::Null);
    assert_eq!(r["emoji"], "🥐");
    assert_eq!(r["source_url"], "https://two.example/beta");

    // Round 2: rotate roles -- set description (previously cleared), clear emoji
    // (previously kept), keep source_url (omitted, previously set). This puts
    // every field through a different role than round 1, so a bind shift among
    // $2 (description), $4 (emoji), $5 (source_url) can't hide behind a field
    // that only ever played one role.
    let r = put_recipe(
        &ctx,
        &id,
        json!({ "description": "Desc-Gamma", "emoji": "" }),
    )
    .await;
    assert_eq!(r["description"], "Desc-Gamma");
    assert_eq!(r["emoji"], serde_json::Value::Null);
    assert_eq!(r["source_url"], "https://two.example/beta");
}

#[tokio::test]
async fn create_normalises_empty_strings_to_null() {
    let ctx = common::TestContext::new().await;
    let auth = ctx.auth_1();
    let mut body = recipe_json();
    body["description"] = json!("");
    body["emoji"] = json!("");
    body["source_url"] = json!("");

    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .with_auth(&auth)
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["description"], serde_json::Value::Null);
    assert_eq!(json["emoji"], serde_json::Value::Null);
    assert_eq!(json["source_url"], serde_json::Value::Null);
}

#[tokio::test]
async fn rejects_non_http_source_url_on_create() {
    let ctx = common::TestContext::new().await;
    let auth = ctx.auth_1();
    let mut body = recipe_json();
    body["title"] = json!("Hostile");
    body["source_url"] = json!("javascript:alert(1)");

    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .with_auth(&auth)
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Nothing was persisted.
    let titles = list_titles(&ctx, "").await;
    assert!(!titles.iter().any(|t| t == "Hostile"), "got {titles:?}");
}

#[tokio::test]
async fn rejects_non_http_source_url_on_update() {
    let ctx = common::TestContext::new().await;
    let id = create_named(&ctx, "Safe", 10).await;
    put_recipe(&ctx, &id, json!({ "source_url": "https://vareni.cz/x" })).await;

    for hostile in [
        "javascript:alert(1)",
        "data:text/html,<script>",
        "not-a-url",
    ] {
        let auth = ctx.auth_1();
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/api/recipes/{id}"))
            .header("Content-Type", "application/json")
            .with_auth(&auth)
            .body(Body::from(json!({ "source_url": hostile }).to_string()))
            .unwrap();
        let resp = ctx.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "input: {hostile}");
    }

    // The good URL survived every rejected attempt.
    let r = put_recipe(&ctx, &id, json!({})).await;
    assert_eq!(r["source_url"], "https://vareni.cz/x");
}

#[tokio::test]
async fn empty_source_url_is_still_accepted_as_clear() {
    let ctx = common::TestContext::new().await;
    let id = create_named(&ctx, "Clearable", 10).await;
    put_recipe(&ctx, &id, json!({ "source_url": "https://vareni.cz/x" })).await;
    let r = put_recipe(&ctx, &id, json!({ "source_url": "" })).await;
    assert_eq!(r["source_url"], serde_json::Value::Null);
}
