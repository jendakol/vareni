//! End-to-end API smoke tests for the multi-section recipe feature.
//!
//! Run with:
//!   cargo test --release --ignored --test api_smoke_test -- --nocapture
//!
//! Test 6 (LLM ingest) additionally requires ANTHROPIC_API_KEY in .env.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn oneshot_json(
    ctx: &common::TestContext,
    method: &str,
    uri: &str,
    auth: (&str, &str),
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(auth.0, auth.1);

    let req_body = if let Some(json) = body {
        builder = builder.header("Content-Type", "application/json");
        Body::from(serde_json::to_string(&json).unwrap())
    } else {
        Body::empty()
    };

    let req = builder.body(req_body).unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

fn single_section_payload() -> serde_json::Value {
    json!({
        "title": "Smoke Test Soup",
        "description": "A basic soup",
        "servings": 2,
        "source_type": "manual",
        "tags": ["soup", "easy"],
        "sections": [
            {
                "label": null,
                "description": null,
                "prep_time_min": 10,
                "cook_time_min": 30,
                "sort_order": 0,
                "ingredients": [
                    { "name": "water",  "amount": 1.0,  "unit": "L",    "note": null },
                    { "name": "carrot", "amount": 200.0,"unit": "g",    "note": "sliced" }
                ],
                "steps": [
                    { "step_order": 1, "instruction": "Boil water" },
                    { "step_order": 2, "instruction": "Add carrot and simmer" }
                ]
            }
        ]
    })
}

// ---------------------------------------------------------------------------
// Test 1 – Create a single-section recipe (default new recipe)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "smoke test: run with --ignored"]
async fn test_01_create_single_section_recipe() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    let (status, body) = oneshot_json(
        &ctx,
        "POST",
        "/api/recipes",
        (&key, &value),
        Some(single_section_payload()),
    )
    .await;

    eprintln!("[test_01] status={status}");
    eprintln!(
        "[test_01] body={}",
        serde_json::to_string_pretty(&body).unwrap()
    );

    assert_eq!(status, StatusCode::CREATED, "expected 201 CREATED");

    let sections = body["sections"].as_array().expect("sections array");
    assert_eq!(
        sections.len(),
        1,
        "expected 1 section, got {}",
        sections.len()
    );

    let s0 = &sections[0];
    assert_eq!(s0["label"], serde_json::Value::Null, "label should be null");
    assert!(s0["id"].is_string(), "section id should be a UUID string");

    // Derived times come from summing section times
    assert_eq!(body["prep_time_min"], 10, "prep_time_min should be 10");
    assert_eq!(body["cook_time_min"], 30, "cook_time_min should be 30");

    let ingredients = s0["ingredients"].as_array().expect("ingredients array");
    assert_eq!(ingredients.len(), 2, "expected 2 ingredients");

    let steps = s0["steps"].as_array().expect("steps array");
    assert_eq!(steps.len(), 2, "expected 2 steps");

    eprintln!(
        "[test_01] PASS — single-section recipe created, id={}",
        body["id"]
    );
}

// ---------------------------------------------------------------------------
// Test 2 – Edit recipe: add a second section, verify sort_order and labels
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "smoke test: run with --ignored"]
async fn test_02_add_second_section() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    // Create
    let (_, created) = oneshot_json(
        &ctx,
        "POST",
        "/api/recipes",
        (&key, &value),
        Some(single_section_payload()),
    )
    .await;
    let recipe_id = created["id"].as_str().unwrap();
    let section1_id = created["sections"][0]["id"].as_str().unwrap();

    eprintln!("[test_02] created recipe_id={recipe_id} section1_id={section1_id}");

    // PUT with 2 sections — move carrot to section 2, keep water in section 1
    let update = json!({
        "title": "Smoke Test Soup (2 sections)",
        "sections": [
            {
                "id": section1_id,
                "label": "Základ",
                "prep_time_min": 5,
                "cook_time_min": 20,
                "sort_order": 0,
                "ingredients": [
                    { "name": "water", "amount": 1.0, "unit": "L", "note": null }
                ],
                "steps": [
                    { "step_order": 1, "instruction": "Boil water" }
                ]
            },
            {
                "label": "Zelenina",
                "prep_time_min": 5,
                "cook_time_min": 15,
                "sort_order": 1,
                "ingredients": [
                    { "name": "carrot", "amount": 200.0, "unit": "g", "note": "sliced" },
                    { "name": "celery", "amount": 100.0, "unit": "g", "note": null }
                ],
                "steps": [
                    { "step_order": 1, "instruction": "Add vegetables and simmer" }
                ]
            }
        ]
    });

    let (status, body) = oneshot_json(
        &ctx,
        "PUT",
        &format!("/api/recipes/{recipe_id}"),
        (&key, &value),
        Some(update),
    )
    .await;

    eprintln!("[test_02] PUT status={status}");
    eprintln!(
        "[test_02] body={}",
        serde_json::to_string_pretty(&body).unwrap()
    );

    assert_eq!(status, StatusCode::OK, "expected 200 OK");

    let sections = body["sections"].as_array().expect("sections array");
    assert_eq!(
        sections.len(),
        2,
        "expected 2 sections, got {}",
        sections.len()
    );

    assert_eq!(sections[0]["label"], "Základ", "section[0] label mismatch");
    assert_eq!(
        sections[1]["label"], "Zelenina",
        "section[1] label mismatch"
    );
    assert_eq!(sections[0]["sort_order"], 0);
    assert_eq!(sections[1]["sort_order"], 1);

    let s0_ings = sections[0]["ingredients"].as_array().unwrap();
    let s1_ings = sections[1]["ingredients"].as_array().unwrap();
    assert_eq!(s0_ings.len(), 1, "section[0] should have 1 ingredient");
    assert_eq!(s1_ings.len(), 2, "section[1] should have 2 ingredients");

    // Derived times: 5+5=10 prep, 20+15=35 cook
    assert_eq!(
        body["prep_time_min"], 10,
        "derived prep_time_min should be 10"
    );
    assert_eq!(
        body["cook_time_min"], 35,
        "derived cook_time_min should be 35"
    );

    eprintln!("[test_02] PASS — 2-section recipe, labels/counts/sort_order correct");
}

// ---------------------------------------------------------------------------
// Test 3 – Toggle "more sections" OFF: PUT back to 1 anonymous section
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "smoke test: run with --ignored"]
async fn test_03_merge_to_single_anonymous_section() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    // Create 2-section recipe
    let two_section = json!({
        "title": "Merge Test",
        "source_type": "manual",
        "sections": [
            {
                "label": "Part A",
                "sort_order": 0,
                "prep_time_min": 5,
                "cook_time_min": null,
                "ingredients": [{ "name": "flour", "amount": 200.0, "unit": "g", "note": null }],
                "steps": [{ "step_order": 1, "instruction": "Mix flour" }]
            },
            {
                "label": "Part B",
                "sort_order": 1,
                "prep_time_min": 3,
                "cook_time_min": 20,
                "ingredients": [{ "name": "eggs", "amount": 2.0, "unit": "pcs", "note": null }],
                "steps": [{ "step_order": 1, "instruction": "Add eggs" }]
            }
        ]
    });

    let (_, created) = oneshot_json(
        &ctx,
        "POST",
        "/api/recipes",
        (&key, &value),
        Some(two_section),
    )
    .await;
    let recipe_id = created["id"].as_str().unwrap();
    eprintln!("[test_03] created recipe_id={recipe_id}");
    assert_eq!(
        created["sections"].as_array().unwrap().len(),
        2,
        "setup: should have 2 sections"
    );

    // PUT with 1 anonymous merged section (no id = fresh insert)
    let merged = json!({
        "sections": [
            {
                "label": null,
                "sort_order": 0,
                "prep_time_min": 8,
                "cook_time_min": 20,
                "ingredients": [
                    { "name": "flour", "amount": 200.0, "unit": "g", "note": null },
                    { "name": "eggs",  "amount": 2.0,   "unit": "pcs","note": null }
                ],
                "steps": [
                    { "step_order": 1, "instruction": "Mix flour" },
                    { "step_order": 2, "instruction": "Add eggs" }
                ]
            }
        ]
    });

    let (status, body) = oneshot_json(
        &ctx,
        "PUT",
        &format!("/api/recipes/{recipe_id}"),
        (&key, &value),
        Some(merged),
    )
    .await;

    eprintln!("[test_03] PUT status={status}");
    eprintln!(
        "[test_03] body={}",
        serde_json::to_string_pretty(&body).unwrap()
    );

    assert_eq!(status, StatusCode::OK, "expected 200 OK");

    let sections = body["sections"].as_array().expect("sections");
    assert_eq!(
        sections.len(),
        1,
        "expected 1 section after merge, got {}",
        sections.len()
    );
    assert_eq!(
        sections[0]["label"],
        serde_json::Value::Null,
        "merged section should have null label"
    );

    let ings = sections[0]["ingredients"].as_array().unwrap();
    assert_eq!(ings.len(), 2, "merged section should have 2 ingredients");

    eprintln!("[test_03] PASS — merged back to 1 anonymous section");
}

// ---------------------------------------------------------------------------
// Test 4 – Delete a section via PUT (omit section 2's id)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "smoke test: run with --ignored"]
async fn test_04_delete_section_via_put() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    // Create 2-section recipe
    let two_section = json!({
        "title": "Delete Section Test",
        "source_type": "manual",
        "sections": [
            {
                "label": "Keep",
                "sort_order": 0,
                "prep_time_min": 5,
                "cook_time_min": 10,
                "ingredients": [{ "name": "onion", "amount": 1.0, "unit": "pcs", "note": null }],
                "steps": [{ "step_order": 1, "instruction": "Chop onion" }]
            },
            {
                "label": "Drop",
                "sort_order": 1,
                "prep_time_min": 5,
                "cook_time_min": 5,
                "ingredients": [{ "name": "garlic", "amount": 2.0, "unit": "cloves", "note": null }],
                "steps": [{ "step_order": 1, "instruction": "Mince garlic" }]
            }
        ]
    });

    let (_, created) = oneshot_json(
        &ctx,
        "POST",
        "/api/recipes",
        (&key, &value),
        Some(two_section),
    )
    .await;
    let recipe_id = created["id"].as_str().unwrap();
    let keep_section_id = created["sections"][0]["id"].as_str().unwrap();
    let drop_section_id = created["sections"][1]["id"].as_str().unwrap();

    eprintln!("[test_04] created recipe_id={recipe_id}");
    eprintln!("[test_04] keep_section_id={keep_section_id}  drop_section_id={drop_section_id}");

    // PUT with only section "Keep" — omit "Drop"
    let update = json!({
        "sections": [
            {
                "id": keep_section_id,
                "label": "Keep",
                "sort_order": 0,
                "prep_time_min": 5,
                "cook_time_min": 10,
                "ingredients": [{ "name": "onion", "amount": 1.0, "unit": "pcs", "note": null }],
                "steps": [{ "step_order": 1, "instruction": "Chop onion" }]
            }
        ]
    });

    let (put_status, put_body) = oneshot_json(
        &ctx,
        "PUT",
        &format!("/api/recipes/{recipe_id}"),
        (&key, &value),
        Some(update),
    )
    .await;

    eprintln!("[test_04] PUT status={put_status}");
    eprintln!(
        "[test_04] PUT body={}",
        serde_json::to_string_pretty(&put_body).unwrap()
    );

    assert_eq!(put_status, StatusCode::OK);
    let sections = put_body["sections"].as_array().unwrap();
    assert_eq!(
        sections.len(),
        1,
        "PUT response should show 1 section, got {}",
        sections.len()
    );
    assert_eq!(
        sections[0]["id"], keep_section_id,
        "remaining section should be the 'Keep' one"
    );
    assert_eq!(sections[0]["label"], "Keep");

    // Confirm with GET
    let (get_status, get_body) = oneshot_json(
        &ctx,
        "GET",
        &format!("/api/recipes/{recipe_id}"),
        (&key, &value),
        None,
    )
    .await;

    eprintln!("[test_04] GET status={get_status}");
    eprintln!(
        "[test_04] GET body={}",
        serde_json::to_string_pretty(&get_body).unwrap()
    );

    assert_eq!(get_status, StatusCode::OK);
    let get_sections = get_body["sections"].as_array().unwrap();
    assert_eq!(
        get_sections.len(),
        1,
        "GET should return 1 section after delete, got {}",
        get_sections.len()
    );

    // Verify the drop_section_id is no longer present
    let remaining_ids: Vec<&str> = get_sections
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert!(
        !remaining_ids.contains(&drop_section_id),
        "dropped section id={drop_section_id} should be gone, but still found in GET response"
    );

    eprintln!("[test_04] PASS — section 'Drop' was deleted via PUT");
}

// ---------------------------------------------------------------------------
// Test 5 – Reject cross-recipe section_id injection
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "smoke test: run with --ignored"]
async fn test_05_reject_cross_recipe_section_id_injection() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    // Create recipe A
    let (_, recipe_a) = oneshot_json(
        &ctx,
        "POST",
        "/api/recipes",
        (&key, &value),
        Some(single_section_payload()),
    )
    .await;
    let recipe_a_id = recipe_a["id"].as_str().unwrap();

    // Create recipe B
    let (_, recipe_b) = oneshot_json(
        &ctx,
        "POST",
        "/api/recipes",
        (&key, &value),
        Some(json!({
            "title": "Recipe B",
            "source_type": "manual",
            "sections": [
                {
                    "label": null,
                    "sort_order": 0,
                    "ingredients": [{ "name": "salt", "amount": 1.0, "unit": "g", "note": null }],
                    "steps": [{ "step_order": 1, "instruction": "Add salt" }]
                }
            ]
        })),
    )
    .await;
    let foreign_section_id = recipe_b["sections"][0]["id"].as_str().unwrap();

    eprintln!("[test_05] recipe_a_id={recipe_a_id}");
    eprintln!("[test_05] foreign_section_id (from recipe B)={foreign_section_id}");

    // Try to PUT recipe A using section_id from recipe B
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

    let (status, body) = oneshot_json(
        &ctx,
        "PUT",
        &format!("/api/recipes/{recipe_a_id}"),
        (&key, &value),
        Some(payload),
    )
    .await;

    eprintln!("[test_05] status={status}");
    eprintln!(
        "[test_05] body={}",
        serde_json::to_string_pretty(&body).unwrap()
    );

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "cross-recipe section_id injection should return 400, got {status}"
    );

    eprintln!("[test_05] PASS — server rejected cross-recipe section_id injection with 400");
}

// ---------------------------------------------------------------------------
// Test 6 – AI ingest URL → multi-section (LLM, slow, optional)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY and network; run manually"]
async fn test_06_ai_ingest_url_multi_section() {
    // Load .env from repo root (one level up from backend/)
    let _ = dotenvy::from_path(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.env"));

    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY missing");

    let client = cooking_app::ai::client::AnthropicClient::new(&api_key);
    let http_client = reqwest::Client::new();

    let url = "https://www.vareni.cz/recepty/svestkovy-kolac-s-tvarohem/";

    eprintln!("[test_06] calling parse_url for {url}");
    let parsed = cooking_app::ai::ingest::parse_url(&client, &http_client, None, url)
        .await
        .expect("parse_url failed");

    eprintln!("[test_06] title: {}", parsed.title);
    eprintln!("[test_06] sections count: {}", parsed.sections.len());
    for (i, s) in parsed.sections.iter().enumerate() {
        eprintln!(
            "[test_06]   section[{i}] label={:?} ingredients={} steps={}",
            s.label,
            s.ingredients.len(),
            s.steps.len()
        );
    }

    assert!(
        parsed.sections.len() >= 3,
        "expected >=3 sections (těsto/náplň/drobenka), got {}",
        parsed.sections.len()
    );

    let label_text = parsed
        .sections
        .iter()
        .filter_map(|s| s.label.as_deref())
        .collect::<Vec<_>>()
        .join(" | ")
        .to_lowercase();

    eprintln!("[test_06] labels: {label_text}");

    assert!(
        label_text.contains("těsto"),
        "no section labelled 'těsto'; labels: {label_text}"
    );
    assert!(
        label_text.contains("náplň")
            || label_text.contains("naplň")
            || label_text.contains("nápln"),
        "no filling section; labels: {label_text}"
    );
    assert!(
        label_text.contains("drobenk"),
        "no streusel section; labels: {label_text}"
    );

    for (i, s) in parsed.sections.iter().enumerate() {
        assert!(
            !s.ingredients.is_empty() || !s.steps.is_empty(),
            "section[{i}] '{}' is empty",
            s.label.as_deref().unwrap_or("")
        );
    }

    eprintln!(
        "[test_06] PASS — AI ingest produced {} sections",
        parsed.sections.len()
    );
}

// ---------------------------------------------------------------------------
// Test 7 – Delete recipe: verify 404 afterwards
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "smoke test: run with --ignored"]
async fn test_07_delete_recipe_then_404() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    // Create
    let (create_status, created) = oneshot_json(
        &ctx,
        "POST",
        "/api/recipes",
        (&key, &value),
        Some(single_section_payload()),
    )
    .await;
    assert_eq!(create_status, StatusCode::CREATED);
    let recipe_id = created["id"].as_str().unwrap();

    eprintln!("[test_07] created recipe_id={recipe_id}");

    // Delete
    let (del_status, del_body) = oneshot_json(
        &ctx,
        "DELETE",
        &format!("/api/recipes/{recipe_id}"),
        (&key, &value),
        None,
    )
    .await;

    eprintln!("[test_07] DELETE status={del_status}");
    eprintln!(
        "[test_07] DELETE body={}",
        serde_json::to_string_pretty(&del_body).unwrap()
    );

    assert_eq!(
        del_status,
        StatusCode::NO_CONTENT,
        "expected 204 NO_CONTENT on delete"
    );

    // Confirm 404
    let (get_status, get_body) = oneshot_json(
        &ctx,
        "GET",
        &format!("/api/recipes/{recipe_id}"),
        (&key, &value),
        None,
    )
    .await;

    eprintln!("[test_07] GET after delete status={get_status}");
    eprintln!(
        "[test_07] GET body={}",
        serde_json::to_string_pretty(&get_body).unwrap()
    );

    assert_eq!(
        get_status,
        StatusCode::NOT_FOUND,
        "expected 404 after delete, got {get_status}"
    );

    eprintln!("[test_07] PASS — recipe deleted, GET returns 404");
}
