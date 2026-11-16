mod common;

use common::WithAuth;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cooking_app::db::images;
use cooking_app::models::ImageKind;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

async fn create_recipe(ctx: &common::TestContext) -> Uuid {
    let body = json!({
        "title": "Img Recipe",
        "servings": 2,
        "source_type": "manual",
        "tags": [],
        "sections": [{
            "label": null, "description": null, "prep_time_min": 5, "cook_time_min": 5,
            "sort_order": 0,
            "ingredients": [{ "name": "water", "amount": 1.0, "unit": "l", "note": null }],
            "steps": [{ "step_order": 1, "instruction": "Boil" }]
        }]
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .with_auth(&ctx.auth_1())
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    json["id"].as_str().unwrap().parse().unwrap()
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn get_json(ctx: &common::TestContext, uri: &str) -> Value {
    let req = Request::builder()
        .uri(uri)
        .with_auth(&ctx.auth_1())
        .body(Body::empty())
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    body_json(resp).await
}

async fn insert(
    ctx: &common::TestContext,
    recipe_id: Uuid,
    kind: ImageKind,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    let mut conn = ctx.pool.acquire().await.unwrap();
    images::insert(&mut conn, id, recipe_id, kind, 800, 600).await?;
    Ok(id)
}

#[tokio::test]
async fn second_cover_violates_unique_index() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    insert(&ctx, recipe_id, ImageKind::Cover).await.unwrap();
    let err = insert(&ctx, recipe_id, ImageKind::Cover).await.unwrap_err();
    let db_err = err.as_database_error().expect("database error");
    assert!(db_err.is_unique_violation());
}

#[tokio::test]
async fn cover_id_appears_in_list_and_detail() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let cover = insert(&ctx, recipe_id, ImageKind::Cover).await.unwrap();
    let shot = insert(&ctx, recipe_id, ImageKind::Screenshot)
        .await
        .unwrap();

    let list = get_json(&ctx, "/api/recipes").await;
    let item = &list["items"][0];
    assert_eq!(item["cover_image_id"], json!(cover));

    let detail = get_json(&ctx, &format!("/api/recipes/{recipe_id}")).await;
    assert_eq!(detail["cover_image_id"], json!(cover));
    assert_eq!(
        detail["screenshots"],
        json!([{ "id": shot, "width": 800, "height": 600 }])
    );

    assert_eq!(images::public_cover(&ctx.pool, "nope").await.unwrap(), None);
    assert!(
        images::get(&ctx.pool, recipe_id, cover)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        images::get(&ctx.pool, Uuid::new_v4(), cover)
            .await
            .unwrap()
            .is_none()
    );
    let mut conn = ctx.pool.acquire().await.unwrap();
    assert_eq!(
        images::lock_cover(&mut conn, recipe_id).await.unwrap(),
        Some(cover)
    );
    let deleted = images::delete(&mut conn, recipe_id, shot).await.unwrap();
    assert_eq!(deleted.unwrap().id, shot);
}

#[tokio::test]
async fn deleting_recipe_cascades_rows() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    insert(&ctx, recipe_id, ImageKind::Cover).await.unwrap();
    assert_eq!(
        images::count_for_recipe(&ctx.pool, recipe_id)
            .await
            .unwrap(),
        1
    );

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/recipes/{recipe_id}"))
        .with_auth(&ctx.auth_1())
        .body(Body::empty())
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    assert_eq!(
        images::count_for_recipe(&ctx.pool, recipe_id)
            .await
            .unwrap(),
        0
    );
}

// ---- HTTP upload / serve / delete ----

const BOUNDARY: &str = "XTESTBOUNDARYX";

fn jpeg_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 100, 50]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
    out.into_inner()
}

fn multipart_body(parts: &[(&str, Option<&str>, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, filename, data) in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        match filename {
            Some(f) => body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{name}\"; filename=\"{f}\"\r\n\
                     Content-Type: application/octet-stream\r\n\r\n"
                )
                .as_bytes(),
            ),
            None => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
            ),
        }
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

async fn post_multipart(
    ctx: &common::TestContext,
    uri: &str,
    parts: &[(&str, Option<&str>, &[u8])],
) -> axum::response::Response {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .with_auth(&ctx.auth_1())
        .body(Body::from(multipart_body(parts)))
        .unwrap();
    ctx.router.clone().oneshot(req).await.unwrap()
}

async fn upload(
    ctx: &common::TestContext,
    recipe_id: Uuid,
    kind: &str,
    files: Vec<Vec<u8>>,
) -> axum::response::Response {
    let mut parts: Vec<(&str, Option<&str>, &[u8])> = vec![("kind", None, kind.as_bytes())];
    for f in &files {
        parts.push(("image", Some("a.jpg"), f));
    }
    post_multipart(ctx, &format!("/api/recipes/{recipe_id}/images"), &parts).await
}

async fn get_raw(
    ctx: &common::TestContext,
    uri: &str,
    auth: Option<&common::TestAuth>,
) -> axum::response::Response {
    let mut b = Request::builder().uri(uri);
    if let Some(a) = auth {
        b = b.with_auth(a);
    }
    ctx.router
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn delete_image(ctx: &common::TestContext, recipe_id: Uuid, id: &str) -> StatusCode {
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/recipes/{recipe_id}/images/{id}"))
        .with_auth(&ctx.auth_1())
        .body(Body::empty())
        .unwrap();
    ctx.router.clone().oneshot(req).await.unwrap().status()
}

fn recipe_dir(ctx: &common::TestContext, recipe_id: Uuid) -> std::path::PathBuf {
    std::path::Path::new(&ctx.config.upload_dir)
        .join("recipes")
        .join(recipe_id.to_string())
}

fn file_path(
    ctx: &common::TestContext,
    recipe_id: Uuid,
    id: &str,
    variant: &str,
) -> std::path::PathBuf {
    recipe_dir(ctx, recipe_id).join(format!("{id}_{variant}.jpg"))
}

fn dir_entries(dir: &std::path::Path) -> Vec<String> {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[tokio::test]
async fn upload_cover_and_fetch_variants() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let resp = upload(&ctx, recipe_id, "cover", vec![jpeg_bytes(800, 600)]).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    assert_eq!(json.as_array().unwrap().len(), 1);
    let id = json[0]["id"].as_str().unwrap().to_string();
    assert_eq!(json[0]["kind"], "cover");

    for variant in ["full", "thumb"] {
        let resp = get_raw(
            &ctx,
            &format!("/api/recipes/{recipe_id}/images/{id}/{variant}"),
            Some(&ctx.auth_1()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK, "{variant}");
        assert_eq!(resp.headers()["content-type"], "image/jpeg");
        assert_eq!(
            resp.headers()["cache-control"],
            "private, max-age=31536000, immutable"
        );
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
        assert_eq!(
            std::fs::read(file_path(&ctx, recipe_id, &id, variant)).unwrap(),
            bytes.to_vec()
        );
    }

    let bad = get_raw(
        &ctx,
        &format!("/api/recipes/{recipe_id}/images/{id}/huge"),
        Some(&ctx.auth_1()),
    )
    .await;
    assert_eq!(bad.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn replacing_cover_removes_old_files() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let first = body_json(upload(&ctx, recipe_id, "cover", vec![jpeg_bytes(300, 200)]).await).await;
    let old = first[0]["id"].as_str().unwrap().to_string();
    let second =
        body_json(upload(&ctx, recipe_id, "cover", vec![jpeg_bytes(300, 200)]).await).await;
    let new = second[0]["id"].as_str().unwrap().to_string();
    assert_ne!(old, new);

    let resp = get_raw(
        &ctx,
        &format!("/api/recipes/{recipe_id}/images/{old}/full"),
        Some(&ctx.auth_1()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert!(!file_path(&ctx, recipe_id, &old, "full").exists());
    assert!(!file_path(&ctx, recipe_id, &old, "thumb").exists());
    assert!(file_path(&ctx, recipe_id, &new, "full").exists());

    let detail = get_json(&ctx, &format!("/api/recipes/{recipe_id}")).await;
    assert_eq!(detail["cover_image_id"], json!(new));
    assert_eq!(dir_entries(&recipe_dir(&ctx, recipe_id)).len(), 2);
}

#[tokio::test]
async fn upload_multiple_screenshots() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let resp = upload(
        &ctx,
        recipe_id,
        "screenshot",
        vec![
            jpeg_bytes(100, 200),
            jpeg_bytes(110, 210),
            jpeg_bytes(120, 220),
        ],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    assert_eq!(json.as_array().unwrap().len(), 3);

    let detail = get_json(&ctx, &format!("/api/recipes/{recipe_id}")).await;
    let shots = detail["screenshots"].as_array().unwrap();
    assert_eq!(shots.len(), 3);
    let widths: Vec<_> = shots.iter().map(|s| s["width"].as_i64().unwrap()).collect();
    assert_eq!(widths, vec![100, 110, 120]);
    assert!(detail["cover_image_id"].is_null());
}

#[tokio::test]
async fn upload_garbage_returns_400() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let resp = upload(&ctx, recipe_id, "cover", vec![b"hello".to_vec()]).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(dir_entries(&recipe_dir(&ctx, recipe_id)).is_empty());
    assert_eq!(
        images::count_for_recipe(&ctx.pool, recipe_id)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn cover_with_two_images_is_400() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let first = body_json(upload(&ctx, recipe_id, "cover", vec![jpeg_bytes(50, 50)]).await).await;
    let old = first[0]["id"].as_str().unwrap().to_string();
    let before = dir_entries(&recipe_dir(&ctx, recipe_id));

    let resp = upload(
        &ctx,
        recipe_id,
        "cover",
        vec![jpeg_bytes(50, 50), jpeg_bytes(50, 50)],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let detail = get_json(&ctx, &format!("/api/recipes/{recipe_id}")).await;
    assert_eq!(detail["cover_image_id"], json!(old));
    let mut after = dir_entries(&recipe_dir(&ctx, recipe_id));
    let mut before = before;
    before.sort();
    after.sort();
    assert_eq!(before, after);
    assert!(file_path(&ctx, recipe_id, &old, "full").exists());
}

#[tokio::test]
async fn cover_with_two_images_stores_nothing_without_prior_cover() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let resp = upload(
        &ctx,
        recipe_id,
        "cover",
        vec![jpeg_bytes(50, 50), jpeg_bytes(50, 50)],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        images::count_for_recipe(&ctx.pool, recipe_id)
            .await
            .unwrap(),
        0
    );
    assert!(dir_entries(&recipe_dir(&ctx, recipe_id)).is_empty());
}

#[tokio::test]
async fn store_completes_when_caller_is_dropped() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let state = cooking_app::AppState {
        pool: ctx.pool.clone(),
        config: std::sync::Arc::new(ctx.config.clone()),
        http_client: reqwest::Client::new(),
        embedding: None,
        browser_semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
        image_semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
        image_fetch_semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
        oidc: std::sync::Arc::new(cooking_app::auth_oidc::Oidc::new().unwrap()),
        image_fetch: cooking_app::safe_fetch::ImageFetcher::new(false).unwrap(),
    };
    let processed = cooking_app::images::process(&jpeg_bytes(60, 40), ImageKind::Cover).unwrap();

    // One poll spawns the detached task; the timeout then drops the caller future.
    let fut = cooking_app::image_store::store(&state, recipe_id, ImageKind::Cover, processed);
    assert!(
        tokio::time::timeout(std::time::Duration::ZERO, fut)
            .await
            .is_err()
    );

    for _ in 0..100 {
        if images::count_for_recipe(&ctx.pool, recipe_id)
            .await
            .unwrap()
            == 1
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        images::count_for_recipe(&ctx.pool, recipe_id)
            .await
            .unwrap(),
        1
    );
    let names = dir_entries(&recipe_dir(&ctx, recipe_id));
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.iter().all(|n| n.ends_with(".jpg")), "{names:?}");
}

#[tokio::test]
async fn missing_kind_is_400() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let img = jpeg_bytes(50, 50);
    let resp = post_multipart(
        &ctx,
        &format!("/api/recipes/{recipe_id}/images"),
        &[("image", Some("a.jpg"), &img)],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = upload(&ctx, recipe_id, "banner", vec![img]).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        images::count_for_recipe(&ctx.pool, recipe_id)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn upload_to_missing_recipe_is_404() {
    let ctx = common::TestContext::new().await;
    let resp = upload(&ctx, Uuid::new_v4(), "cover", vec![jpeg_bytes(50, 50)]).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn image_of_other_recipe_is_404() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let other = create_recipe(&ctx).await;
    let json = body_json(upload(&ctx, recipe_id, "cover", vec![jpeg_bytes(50, 50)]).await).await;
    let id = json[0]["id"].as_str().unwrap();

    let ok = get_raw(
        &ctx,
        &format!("/api/recipes/{recipe_id}/images/{id}/full"),
        Some(&ctx.auth_1()),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK);
    let resp = get_raw(
        &ctx,
        &format!("/api/recipes/{other}/images/{id}/full"),
        Some(&ctx.auth_1()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(delete_image(&ctx, other, id).await, StatusCode::NOT_FOUND);
    assert!(file_path(&ctx, recipe_id, id, "full").exists());
}

#[tokio::test]
async fn unauthenticated_get_is_401() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let json = body_json(upload(&ctx, recipe_id, "cover", vec![jpeg_bytes(50, 50)]).await).await;
    let id = json[0]["id"].as_str().unwrap();
    let uri = format!("/api/recipes/{recipe_id}/images/{id}/full");

    assert_eq!(
        get_raw(&ctx, &uri, Some(&ctx.auth_1())).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        get_raw(&ctx, &uri, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn unauthenticated_upload_is_401() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let img = jpeg_bytes(50, 50);
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/recipes/{recipe_id}/images"))
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(multipart_body(&[
            ("kind", None, b"cover"),
            ("image", Some("a.jpg"), &img),
        ])))
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert!(dir_entries(&recipe_dir(&ctx, recipe_id)).is_empty());
}

#[tokio::test]
async fn delete_image_removes_row_and_files() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let json =
        body_json(upload(&ctx, recipe_id, "screenshot", vec![jpeg_bytes(50, 80)]).await).await;
    let id = json[0]["id"].as_str().unwrap().to_string();
    assert!(file_path(&ctx, recipe_id, &id, "full").exists());

    assert_eq!(
        delete_image(&ctx, recipe_id, &id).await,
        StatusCode::NO_CONTENT
    );
    let resp = get_raw(
        &ctx,
        &format!("/api/recipes/{recipe_id}/images/{id}/full"),
        Some(&ctx.auth_1()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert!(!file_path(&ctx, recipe_id, &id, "full").exists());
    assert!(!file_path(&ctx, recipe_id, &id, "thumb").exists());
    assert_eq!(
        delete_image(&ctx, recipe_id, &id).await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn delete_recipe_removes_files() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    upload(&ctx, recipe_id, "cover", vec![jpeg_bytes(50, 50)]).await;
    assert!(recipe_dir(&ctx, recipe_id).exists());

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/recipes/{recipe_id}"))
        .with_auth(&ctx.auth_1())
        .body(Body::empty())
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert!(!recipe_dir(&ctx, recipe_id).exists());
}

#[tokio::test]
async fn uploads_path_not_served() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let json = body_json(upload(&ctx, recipe_id, "cover", vec![jpeg_bytes(50, 50)]).await).await;
    let id = json[0]["id"].as_str().unwrap();
    let on_disk = std::fs::read(file_path(&ctx, recipe_id, id, "full")).unwrap();

    let resp = get_raw(
        &ctx,
        &format!("/uploads/recipes/{recipe_id}/{id}_full.jpg"),
        Some(&ctx.auth_1()),
    )
    .await;
    let content_type = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_ne!(bytes.to_vec(), on_disk);
    assert_ne!(content_type.as_deref(), Some("image/jpeg"));
}

#[tokio::test]
async fn ingest_accepts_body_over_2mb() {
    let ctx = common::TestContext::new().await;
    let big = vec![0u8; 3 * 1024 * 1024];
    let resp = post_multipart(
        &ctx,
        "/api/ingest",
        &[
            ("source_type", None, b"unknown"),
            ("image", Some("big.jpg"), &big),
        ],
    )
    .await;
    assert_ne!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    // The handler maps a body-limit hit to 400 as well, so only reaching the
    // source_type dispatch proves the whole part was read.
    let msg = body_json(resp).await["error"].as_str().unwrap().to_string();
    assert!(msg.contains("unknown source_type"), "got: {msg}");
}

#[tokio::test]
async fn ingest_rejects_photo_over_20mb() {
    let ctx = common::TestContext::new().await;
    let big = vec![0u8; 20 * 1024 * 1024 + 1];
    let resp = post_multipart(
        &ctx,
        "/api/ingest",
        &[
            ("source_type", None, b"photo"),
            ("image", Some("big.jpg"), &big),
        ],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let msg = body_json(resp).await["error"].as_str().unwrap().to_string();
    assert_eq!(msg, "Obrázek je příliš velký");
}

#[tokio::test]
async fn upload_rejects_overlong_kind() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe(&ctx).await;
    let resp = upload(&ctx, recipe_id, &"x".repeat(100), vec![jpeg_bytes(50, 50)]).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

mod safe_fetch {
    use cooking_app::images::MAX_INPUT_BYTES;
    use cooking_app::safe_fetch::{FetchError, ImageFetcher};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn server_with_image() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/x.jpg"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1u8; 16]))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn rejects_loopback_literal() {
        let server = server_with_image().await;
        let f = ImageFetcher::new(false).unwrap();
        let res = f.fetch_image(&format!("{}/x.jpg", server.uri())).await;
        assert_eq!(res, Err(FetchError::Forbidden));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn rejects_hostname_resolving_to_loopback() {
        let server = server_with_image().await;
        let port = server.address().port();
        let f = ImageFetcher::new(false).unwrap();
        let res = f
            .fetch_image(&format!("http://localhost:{port}/x.jpg"))
            .await;
        assert_eq!(res, Err(FetchError::Forbidden));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn fetches_body_when_allowed() {
        let server = server_with_image().await;
        let f = ImageFetcher::new(true).unwrap();
        let body = f
            .fetch_image(&format!("{}/x.jpg", server.uri()))
            .await
            .unwrap();
        assert_eq!(body.len(), 16);
    }

    #[tokio::test]
    async fn follows_limited_redirects() {
        let server = server_with_image().await;
        Mock::given(method("GET"))
            .and(path("/r"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", "/x.jpg"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/loop"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", "/loop"))
            .mount(&server)
            .await;
        let f = ImageFetcher::new(true).unwrap();
        assert!(f.fetch_image(&format!("{}/r", server.uri())).await.is_ok());
        let res = f.fetch_image(&format!("{}/loop", server.uri())).await;
        assert!(matches!(res, Err(FetchError::Http(_))), "{res:?}");
    }

    #[tokio::test]
    async fn non_success_status_is_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let f = ImageFetcher::new(true).unwrap();
        let res = f.fetch_image(&format!("{}/x", server.uri())).await;
        assert_eq!(res, Err(FetchError::Http("status 404".into())));
    }

    #[tokio::test]
    async fn enforces_size_cap() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; MAX_INPUT_BYTES + 1]))
            .mount(&server)
            .await;
        let f = ImageFetcher::new(true).unwrap();
        let res = f.fetch_image(&format!("{}/big", server.uri())).await;
        assert_eq!(res, Err(FetchError::TooLarge));
    }

    /// Raw server answering every connection with `head` then `body_len` zero bytes.
    async fn raw_server(head: &'static str, body_len: usize, chunked: bool) -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = sock.read(&mut buf).await;
                    let _ = sock.write_all(head.as_bytes()).await;
                    let block = vec![0u8; 64 * 1024];
                    let mut left = body_len;
                    while left > 0 {
                        let n = left.min(block.len());
                        if chunked {
                            let _ = sock.write_all(format!("{n:x}\r\n").as_bytes()).await;
                        }
                        if sock.write_all(&block[..n]).await.is_err() {
                            return;
                        }
                        if chunked {
                            let _ = sock.write_all(b"\r\n").await;
                        }
                        left -= n;
                    }
                    if chunked {
                        let _ = sock.write_all(b"0\r\n\r\n").await;
                    }
                    let _ = sock.shutdown().await;
                });
            }
        });
        port
    }

    #[tokio::test]
    async fn enforces_size_cap_while_streaming() {
        let port = raw_server(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n",
            MAX_INPUT_BYTES + 1,
            true,
        )
        .await;
        let f = ImageFetcher::new(true).unwrap();
        let res = f.fetch_image(&format!("http://127.0.0.1:{port}/x")).await;
        assert_eq!(res, Err(FetchError::TooLarge));
    }

    #[tokio::test]
    async fn accepts_body_of_exactly_max_size() {
        let port = raw_server(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n",
            MAX_INPUT_BYTES,
            true,
        )
        .await;
        let f = ImageFetcher::new(true).unwrap();
        let body = f
            .fetch_image(&format!("http://127.0.0.1:{port}/x"))
            .await
            .unwrap();
        assert_eq!(body.len(), MAX_INPUT_BYTES);
    }

    async fn redirect_chain(total: usize) -> MockServer {
        let server = MockServer::start().await;
        for i in 0..total {
            let next = if i + 1 == total {
                "/final".to_string()
            } else {
                format!("/h{}", i + 1)
            };
            Mock::given(method("GET"))
                .and(path(format!("/h{i}")))
                .respond_with(ResponseTemplate::new(302).insert_header("Location", next))
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/final"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1u8; 4]))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn three_redirects_succeed_four_fail() {
        let f = ImageFetcher::new(true).unwrap();
        let ok = redirect_chain(3).await;
        assert!(f.fetch_image(&format!("{}/h0", ok.uri())).await.is_ok());
        let bad = redirect_chain(4).await;
        let res = f.fetch_image(&format!("{}/h0", bad.uri())).await;
        assert!(matches!(res, Err(FetchError::Http(_))), "{res:?}");
    }
}

mod cover_url {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn png_bytes() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(40, 30, image::Rgb([200, 30, 30]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    async fn post_recipe(ctx: &common::TestContext, cover_url: &str) -> Value {
        let body = json!({
            "title": "Cover Recipe",
            "servings": 2,
            "source_type": "url",
            "tags": [],
            "cover_image_url": cover_url,
            "sections": [{
                "label": null, "description": null, "prep_time_min": 5, "cook_time_min": 5,
                "sort_order": 0,
                "ingredients": [{ "name": "water", "amount": 1.0, "unit": "l", "note": null }],
                "steps": [{ "step_order": 1, "instruction": "Boil" }]
            }]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/recipes")
            .header("Content-Type", "application/json")
            .with_auth(&ctx.auth_1())
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = ctx.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        body_json(resp).await
    }

    #[tokio::test]
    async fn create_with_cover_url_stores_cover() {
        let ctx = common::TestContext::with_config(|c| c.image_fetch_allow_private = true).await;
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/c.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes()))
            .mount(&server)
            .await;
        let json = post_recipe(&ctx, &format!("{}/c.png", server.uri())).await;
        let cover_id = json["cover_image_id"].as_str().expect("cover set");
        let recipe_id = json["id"].as_str().unwrap();
        let resp = get_raw(
            &ctx,
            &format!("/api/recipes/{recipe_id}/images/{cover_id}/thumb"),
            Some(&ctx.auth_1()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn create_with_unreachable_cover_still_creates() {
        let ctx = common::TestContext::with_config(|c| c.image_fetch_allow_private = true).await;
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let json = post_recipe(&ctx, &format!("{}/c.png", server.uri())).await;
        assert!(json["cover_image_id"].is_null());
    }

    #[tokio::test]
    async fn create_with_private_cover_url_ignored() {
        let ctx = common::TestContext::new().await;
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes()))
            .mount(&server)
            .await;
        let json = post_recipe(&ctx, &format!("{}/c.png", server.uri())).await;
        assert!(json["cover_image_id"].is_null());
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_skips_cover_when_fetch_queue_is_full() {
        let mut ctx =
            common::TestContext::with_config(|c| c.image_fetch_allow_private = true).await;
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes()))
            .mount(&server)
            .await;
        let state = cooking_app::AppState {
            pool: ctx.pool.clone(),
            config: std::sync::Arc::new(ctx.config.clone()),
            http_client: reqwest::Client::new(),
            embedding: None,
            browser_semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
            image_semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
            image_fetch_semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(0)),
            oidc: std::sync::Arc::new(cooking_app::auth_oidc::Oidc::new().unwrap()),
            image_fetch: cooking_app::safe_fetch::ImageFetcher::new(true).unwrap(),
        };
        ctx.router = cooking_app::create_router(state);
        let json = post_recipe(&ctx, &format!("{}/c.png", server.uri())).await;
        assert!(json["cover_image_id"].is_null());
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
