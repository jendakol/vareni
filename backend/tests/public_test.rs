mod common;

use common::WithAuth;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

async fn create_and_share(ctx: &common::TestContext) -> (String, String) {
    // Create a recipe
    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .with_auth(&auth)
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
        .with_auth(&auth)
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
    let auth = ctx.auth_1();
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/recipes/{recipe_id}/share"))
        .with_auth(&auth)
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

// ---- Public DTO and cover ----

const BOUNDARY: &str = "XTESTBOUNDARYX";

fn jpeg_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 100, 50]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
    out.into_inner()
}

async fn upload(ctx: &common::TestContext, recipe_id: &str, kind: &str) -> serde_json::Value {
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"kind\"\r\n\r\n{kind}\r\n\
             --{BOUNDARY}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"a.jpg\"\r\n\
             Content-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(&jpeg_bytes(300, 200));
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/recipes/{recipe_id}/images"))
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .with_auth(&ctx.auth_1())
        .body(Body::from(body))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()[0].clone()
}

async fn get_public(
    ctx: &common::TestContext,
    uri: &str,
    if_none_match: Option<&str>,
) -> axum::response::Response {
    let mut b = Request::builder().uri(uri);
    if let Some(v) = if_none_match {
        b = b.header("If-None-Match", v);
    }
    ctx.router
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn unshare(ctx: &common::TestContext, recipe_id: &str) {
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/recipes/{recipe_id}/share"))
        .with_auth(&ctx.auth_1())
        .body(Body::empty())
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn public_recipe_hides_owner_and_screenshots() {
    let ctx = common::TestContext::new().await;
    let (recipe_id, slug) = create_and_share(&ctx).await;
    let cover = upload(&ctx, &recipe_id, "cover").await;
    upload(&ctx, &recipe_id, "screenshot").await;

    let resp = get_public(&ctx, &format!("/api/public/recipes/{slug}"), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let obj = json.as_object().unwrap();
    for hidden in [
        "owner_id",
        "screenshots",
        "status",
        "discovery_score",
        "discovered_at",
        "scored_at",
        "canonical_name",
    ] {
        assert!(!obj.contains_key(hidden), "{hidden} leaked");
    }
    assert_eq!(json["cover_image_id"], cover["id"]);
    assert_eq!(json["title"], "Shared Recipe");
}

#[tokio::test]
async fn public_cover_served_with_no_cache_and_etag() {
    let ctx = common::TestContext::new().await;
    let (recipe_id, slug) = create_and_share(&ctx).await;
    let cover = upload(&ctx, &recipe_id, "cover").await;
    let id = cover["id"].as_str().unwrap();

    for variant in ["full", "thumb"] {
        let resp = get_public(
            &ctx,
            &format!("/api/public/recipes/{slug}/cover/{variant}"),
            None,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers()["cache-control"], "no-cache");
        assert_eq!(resp.headers()["content-type"], "image/jpeg");
        assert_eq!(resp.headers()["etag"], format!("\"{id}-{variant}\""));
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
    }

    let resp = get_public(
        &ctx,
        &format!("/api/public/recipes/{slug}/cover/huge"),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn public_cover_304_on_matching_etag() {
    let ctx = common::TestContext::new().await;
    let (recipe_id, slug) = create_and_share(&ctx).await;
    let id = upload(&ctx, &recipe_id, "cover").await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let uri = format!("/api/public/recipes/{slug}/cover/full");
    let etag = format!("\"{id}-full\"");

    for inm in [
        etag.clone(),
        format!("W/{etag}"),
        format!("\"other\", {etag}"),
        "*".to_string(),
    ] {
        let resp = get_public(&ctx, &uri, Some(&inm)).await;
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED, "{inm}");
        assert_eq!(resp.headers()["etag"], etag.as_str());
        assert_eq!(resp.headers()["cache-control"], "no-cache");
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        assert!(bytes.is_empty());
    }

    let resp = get_public(&ctx, &uri, Some("\"stale-full\"")).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn public_cover_404_after_unshare() {
    let ctx = common::TestContext::new().await;
    let (recipe_id, slug) = create_and_share(&ctx).await;
    upload(&ctx, &recipe_id, "cover").await;
    let uri = format!("/api/public/recipes/{slug}/cover/full");
    assert_eq!(get_public(&ctx, &uri, None).await.status(), StatusCode::OK);

    unshare(&ctx, &recipe_id).await;
    assert_eq!(
        get_public(&ctx, &uri, None).await.status(),
        StatusCode::NOT_FOUND
    );
    // Revoked access must not 304 either.
    assert_eq!(
        get_public(&ctx, &uri, Some("*")).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn public_cover_404_without_cover() {
    let ctx = common::TestContext::new().await;
    let (recipe_id, slug) = create_and_share(&ctx).await;
    upload(&ctx, &recipe_id, "screenshot").await;
    let resp = get_public(
        &ctx,
        &format!("/api/public/recipes/{slug}/cover/full"),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn public_cover_etag_changes_on_replace() {
    let ctx = common::TestContext::new().await;
    let (recipe_id, slug) = create_and_share(&ctx).await;
    let uri = format!("/api/public/recipes/{slug}/cover/full");
    upload(&ctx, &recipe_id, "cover").await;
    let first = get_public(&ctx, &uri, None).await.headers()["etag"].clone();
    upload(&ctx, &recipe_id, "cover").await;
    let second = get_public(&ctx, &uri, None).await.headers()["etag"].clone();
    assert_ne!(first, second);

    let resp = get_public(&ctx, &uri, Some(first.to_str().unwrap())).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn public_has_no_screenshot_route() {
    let ctx = common::TestContext::new().await;
    let (recipe_id, slug) = create_and_share(&ctx).await;
    let shot = upload(&ctx, &recipe_id, "screenshot").await;
    let id = shot["id"].as_str().unwrap();
    let resp = get_public(
        &ctx,
        &format!("/api/public/recipes/{slug}/images/{id}/full"),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
