mod common;

use common::WithAuth;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

async fn send(router: &Router, req: Request<Body>) -> Response<Body> {
    router.clone().oneshot(req).await.unwrap()
}

async fn json_body(resp: Response<Body>) -> serde_json::Value {
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

fn get(uri: &str) -> axum::http::request::Builder {
    Request::builder().method("GET").uri(uri)
}

fn add_restriction() -> axum::http::request::Builder {
    Request::builder()
        .method("POST")
        .uri("/api/settings/restrictions")
        .header("Content-Type", "application/json")
}

fn restriction_body() -> Body {
    Body::from(json!({ "restriction": "vegan" }).to_string())
}

fn location(resp: &Response<Body>) -> &str {
    resp.headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
}

fn set_cookies(resp: &Response<Body>) -> Vec<String> {
    resp.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect()
}

async fn session_count(ctx: &common::TestContext) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&ctx.pool)
        .await
        .unwrap()
}

fn logout_request(cookie: Option<&str>, body: &str) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .header("Content-Type", "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        b = b.header("Cookie", cookie);
    }
    b.body(Body::from(body.to_owned())).unwrap()
}

fn assert_clears_session_cookie(resp: &Response<Body>) {
    let cookies = set_cookies(resp);
    assert!(
        cookies
            .iter()
            .any(|c| c.starts_with("__Host-vareni_session=;")
                && c.contains("Max-Age=0")
                && c.contains("Secure")
                && c.contains("Path=/")),
        "session cookie not cleared: {cookies:?}"
    );
}

#[tokio::test]
async fn me_without_cookie_is_401() {
    let ctx = common::TestContext::new().await;
    let resp = send(
        &ctx.router,
        get("/api/auth/me").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert!(json_body(resp).await["error"].is_string());
}

#[tokio::test]
async fn bearer_token_never_authenticates_session_routes() {
    let ctx = common::TestContext::new().await;
    for token in ["invalid-garbage-token", "test-log-token"] {
        let req = get("/api/auth/me")
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            send(&ctx.router, req).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
}

#[tokio::test]
async fn me_with_valid_session() {
    let ctx = common::TestContext::new().await;
    let auth = ctx.auth_1();
    let resp = send(
        &ctx.router,
        get("/api/auth/me")
            .header("Cookie", &auth.cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let json = json_body(resp).await;
    assert_eq!(json["id"], ctx.user1_id.to_string());
    assert_eq!(json["name"], "Test User 1");
    assert_eq!(json["email"], "user1@test.com");
    assert_eq!(json["csrf_token"], auth.csrf);
    assert!(json["dietary_restrictions"].is_array());
    assert!(json["food_preferences"].is_array());
    assert!(json.get("external_id").is_none());
    assert!(json.get("password_hash").is_none());
}

#[tokio::test]
async fn unknown_session_is_401() {
    let ctx = common::TestContext::new().await;
    let req = get("/api/auth/me")
        .header("Cookie", "__Host-vareni_session=not-a-real-session")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&ctx.router, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn expired_session_is_401() {
    let ctx = common::TestContext::new().await;
    let expired = common::create_session(&ctx.pool, ctx.user1_id, -1).await;
    let req = get("/api/auth/me")
        .with_auth(&expired)
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&ctx.router, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn duplicate_session_cookie_is_400() {
    let ctx = common::TestContext::new().await;
    let cookie = format!("{}; {}", ctx.user1_auth.cookie, ctx.user2_auth.cookie);
    let req = get("/api/auth/me")
        .header("Cookie", cookie)
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&ctx.router, req).await.status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn non_prefixed_cookie_is_ignored_in_secure_mode() {
    let ctx = common::TestContext::new().await;
    let raw = ctx.user1_auth.cookie.split_once('=').unwrap().1;
    let req = get("/api/auth/me")
        .header("Cookie", format!("vareni_session={raw}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&ctx.router, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn mutation_without_csrf_is_403() {
    let ctx = common::TestContext::new().await;
    let req = add_restriction()
        .header("Cookie", &ctx.user1_auth.cookie)
        .body(restriction_body())
        .unwrap();
    let resp = send(&ctx.router, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(json_body(resp).await["error"].is_string());
}

#[tokio::test]
async fn mutation_with_wrong_csrf_is_403() {
    let ctx = common::TestContext::new().await;
    for csrf in ["wrong", "", ctx.user2_auth.csrf.as_str()] {
        let req = add_restriction()
            .header("Cookie", &ctx.user1_auth.cookie)
            .header("X-CSRF-Token", csrf)
            .body(restriction_body())
            .unwrap();
        assert_eq!(
            send(&ctx.router, req).await.status(),
            StatusCode::FORBIDDEN,
            "csrf {csrf:?}"
        );
    }
}

#[tokio::test]
async fn mutation_with_valid_csrf_succeeds() {
    let ctx = common::TestContext::new().await;
    let req = add_restriction()
        .with_auth(&ctx.user1_auth)
        .body(restriction_body())
        .unwrap();
    assert_eq!(send(&ctx.router, req).await.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn list_users_hides_secrets() {
    let ctx = common::TestContext::new().await;
    let resp = send(
        &ctx.router,
        get("/api/auth/users")
            .with_auth(&ctx.user1_auth)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    let users = json.as_array().unwrap();
    assert_eq!(users.len(), 2);
    for user in users {
        assert!(user["name"].is_string());
        assert!(user.get("external_id").is_none(), "{user}");
        assert!(user.get("password_hash").is_none(), "{user}");
    }
}

#[tokio::test]
async fn sessions_rejected_when_oidc_not_configured() {
    let ctx = common::TestContext::new().await;
    let mut config = ctx.config.clone();
    config.oidc = None;
    let router = common::build_router(&ctx.pool, &config);

    let req = get("/api/auth/me")
        .with_auth(&ctx.user1_auth)
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&router, req).await.status(), StatusCode::UNAUTHORIZED);

    let resp = send(&router, get("/api/auth/login").body(Body::empty()).unwrap()).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert_eq!(location(&resp), "/login?error=idp_unavailable");
}

#[tokio::test]
async fn login_with_unreachable_idp_redirects_with_error() {
    let ctx = common::TestContext::new().await;
    let resp = send(
        &ctx.router,
        get("/api/auth/login?return_url=/plan")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert_eq!(location(&resp), "/login?error=idp_unavailable");
}

#[tokio::test]
async fn callback_without_state_cookie_redirects_state_expired() {
    let ctx = common::TestContext::new().await;
    let resp = send(
        &ctx.router,
        get("/api/auth/callback?code=abc&state=xyz")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert_eq!(location(&resp), "/login?error=state_expired");
}

#[tokio::test]
async fn callback_with_idp_error_redirects_login_failed() {
    let ctx = common::TestContext::new().await;
    let resp = send(
        &ctx.router,
        get("/api/auth/callback?error=access_denied&state=xyz")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert_eq!(location(&resp), "/login?error=login_failed");
    assert!(
        set_cookies(&resp)
            .iter()
            .any(|c| c.starts_with("vareni_oidc_state=;") && c.contains("Max-Age=0"))
    );
}

#[tokio::test]
async fn logout_deletes_session() {
    let ctx = common::TestContext::new().await;
    let auth = ctx.auth_1();
    assert_eq!(session_count(&ctx).await, 2);

    let resp = send(
        &ctx.router,
        logout_request(Some(&auth.cookie), &format!("csrf_token={}", auth.csrf)),
    )
    .await;
    assert!(resp.status().is_redirection());
    // No ID token on this session => local logout only.
    assert_eq!(location(&resp), "/login");
    assert_clears_session_cookie(&resp);
    assert_eq!(session_count(&ctx).await, 1);

    let req = get("/api/auth/me")
        .with_auth(&auth)
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&ctx.router, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn logout_without_session_still_clears_cookie() {
    let ctx = common::TestContext::new().await;
    let resp = send(&ctx.router, logout_request(None, "csrf_token=x")).await;
    assert!(resp.status().is_redirection());
    assert_eq!(location(&resp), "/login");
    assert_clears_session_cookie(&resp);
}

#[tokio::test]
async fn logout_with_bad_csrf_keeps_session_but_clears_cookie() {
    let ctx = common::TestContext::new().await;
    let auth = ctx.auth_1();
    for body in ["csrf_token=wrong", "", "garbage"] {
        let resp = send(&ctx.router, logout_request(Some(&auth.cookie), body)).await;
        assert!(resp.status().is_redirection(), "body {body:?}");
        assert_eq!(location(&resp), "/login");
        assert_clears_session_cookie(&resp);
    }
    assert_eq!(session_count(&ctx).await, 2);
}

#[tokio::test]
async fn expired_sessions_are_cleaned_up() {
    let ctx = common::TestContext::new().await;
    common::create_session(&ctx.pool, ctx.user1_id, -1).await;
    assert_eq!(session_count(&ctx).await, 3);
    let deleted = cooking_app::db::sessions::delete_expired(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(deleted, 1);
    assert_eq!(session_count(&ctx).await, 2);
}

#[tokio::test]
async fn session_id_is_stored_hashed() {
    let ctx = common::TestContext::new().await;
    let raw = ctx.user1_auth.cookie.split_once('=').unwrap().1.to_owned();
    let stored: Vec<String> = sqlx::query_scalar("SELECT id_hash FROM sessions")
        .fetch_all(&ctx.pool)
        .await
        .unwrap();
    assert!(!stored.contains(&raw));
    assert!(stored.contains(&cooking_app::db::sessions::hash_id(&raw)));
}
