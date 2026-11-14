//! End-to-end OIDC login against a wiremock IdP (discovery, token endpoint, JWKS).

mod common;

use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY_A_PEM: &str = include_str!("fixtures/oidc_key_a.pem");
const KEY_A_KID: &str = "test-key-a";
const KEY_A_N: &str = "ybOe8tsKNW5Jb2RGqQ6CrG-fJiy8XddNlEnyPW-RJl5qe4t08aezK2__zS5QH7fp3OZ2-t9dZ5QbOwQ1gOUFkumOCwztLQ6rxGFNqtx-2zzezRjNHIPj_oPnlW-XmgH0tZMvV6FDk8ETMGQjGVNq4t5xRvcllGrLjQfdDhVb_SG07oY_nZ7yJ7rcijTXvlBFpeP4NjMIPeEL2q0cVcOasStl8KDbvhNVJ1hLD0hE1ZvTixjk68V2FH81ehky2BUeNTTCKotem2HQIWmwMo-ImA1Iaje0IeK5eVWUsSh01wq5Hs6KtubBiKZwOmC67e3PKR1l3M2_YrpM9PqosukaJw";
const KEY_B_PEM: &str = include_str!("fixtures/oidc_key_b.pem");
const KEY_B_KID: &str = "test-key-b";
const KEY_B_N: &str = "rHReWjO65mHGgrjk3LgusyqHgfnHXzcijlZPMiTdUOHBZaUb8nKP-IMsGcPQcjmRPe_ZfdXpO-dFhagEJ7tCJGWePgOuiv6iM5uQuvgdJfwPfUzbDFQsFbXuviRS4DeVt0Ir_1Iqbj_xT17XANeIozAozRR3W6rTI4bLSap-UhxeYv2QfIIp-Ff-pMZpekuYviKKosi7hEaL3_C1FVLJf_soyWQWQdS45Obvu0x2EAu7OttPYvOw4ZFU4UX232qOmFgIayZDiQi9gjxzw6gkZLz0A9F3l6pZ2xsKJ3vYqqih8Wley2wZm8IOAkOkLECW8KUeAyEQHE57VPxnWcAMtw";

fn jwk(kid: &str, n: &str) -> Value {
    json!({ "kty": "RSA", "use": "sig", "alg": "RS256", "kid": kid, "n": n, "e": "AQAB" })
}

fn jwks(keys: &[(&str, &str)]) -> Value {
    json!({ "keys": keys.iter().map(|(kid, n)| jwk(kid, n)).collect::<Vec<_>>() })
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn sign(claims: &Value, pem: &str, kid: &str) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.into());
    let key = EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();
    jsonwebtoken::encode(&header, claims, &key).unwrap()
}

struct Idp {
    server: MockServer,
    ctx: common::TestContext,
}

/// Login state parsed from the state cookie.
struct Login {
    cookie: String,
    state: String,
    nonce: String,
}

impl Idp {
    async fn start() -> Self {
        Self::start_with_jwks(jwks(&[(KEY_A_KID, KEY_A_N)])).await
    }

    async fn start_with_jwks(initial_jwks: Value) -> Self {
        let server = MockServer::start().await;
        let issuer = server.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/auth"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
                "end_session_endpoint": format!("{issuer}/logout"),
                "response_types_supported": ["code"],
                "subject_types_supported": ["public"],
                "id_token_signing_alg_values_supported": ["RS256"],
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(initial_jwks))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(jwks(&[(KEY_A_KID, KEY_A_N), (KEY_B_KID, KEY_B_N)])),
            )
            .with_priority(2)
            .mount(&server)
            .await;

        let ctx = common::TestContext::with_issuer(&issuer).await;
        Self { server, ctx }
    }

    fn issuer(&self) -> String {
        self.server.uri()
    }

    async fn send(&self, req: Request<Body>) -> Response<Body> {
        self.ctx.router.clone().oneshot(req).await.unwrap()
    }

    async fn login(&self, return_url: &str) -> Login {
        let resp = self
            .send(
                Request::get(format!("/api/auth/login?return_url={return_url}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::FOUND);
        let location = location(&resp);
        assert!(
            location.starts_with(&format!("{}/auth?", self.issuer())),
            "{location}"
        );
        assert!(location.contains("code_challenge_method=S256"));
        assert!(location.contains("client_id=vareni"));

        let set_cookie = set_cookies(&resp)
            .into_iter()
            .find(|c| c.starts_with("vareni_oidc_state="))
            .expect("state cookie");
        assert!(set_cookie.contains("Path=/api/auth"));
        assert!(set_cookie.contains("HttpOnly"));
        assert!(set_cookie.contains("SameSite=Lax"));
        assert!(set_cookie.contains("Secure"));
        let cookie = set_cookie.split(';').next().unwrap().to_owned();
        let value = cookie.split_once('=').unwrap().1;
        let parts: Vec<&str> = value.splitn(4, ':').collect();
        Login {
            state: parts[0].to_owned(),
            nonce: parts[2].to_owned(),
            cookie,
        }
    }

    fn claims(&self, login: &Login, sub: &str) -> Value {
        json!({
            "iss": self.issuer(),
            "aud": "vareni",
            "sub": sub,
            "exp": now() + 300,
            "iat": now(),
            "nonce": login.nonce,
            "email": "user1@test.com",
            "roles": ["offline_access", "vareni"],
        })
    }

    async fn mount_token(&self, id_token: &str) {
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "test-access-token",
                "token_type": "Bearer",
                "expires_in": 300,
                "id_token": id_token,
            })))
            .mount(&self.server)
            .await;
    }

    async fn callback(&self, login: &Login, state: &str) -> Response<Body> {
        self.send(
            Request::get(format!("/api/auth/callback?code=test-code&state={state}"))
                .header("Cookie", &login.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Full flow with the given (already signed) ID token.
    async fn callback_with_token(&self, login: &Login, id_token: &str) -> Response<Body> {
        self.mount_token(id_token).await;
        self.callback(login, &login.state).await
    }
}

fn location(resp: &Response<Body>) -> String {
    resp.headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned()
}

fn set_cookies(resp: &Response<Body>) -> Vec<String> {
    resp.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect()
}

fn session_cookie(resp: &Response<Body>) -> Option<String> {
    set_cookies(resp)
        .into_iter()
        .find(|c| c.starts_with("__Host-vareni_session=") && !c.contains("Max-Age=0"))
}

fn assert_login_error(resp: &Response<Body>, code: &str) {
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert_eq!(location(resp), format!("/login?error={code}"));
    assert!(session_cookie(resp).is_none());
}

async fn me(idp: &Idp, cookie: &str) -> Response<Body> {
    idp.send(
        Request::get("/api/auth/me")
            .header("Cookie", cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn successful_login_creates_session() {
    let idp = Idp::start().await;
    let login = idp.login("/plan").await;
    let token = sign(&idp.claims(&login, common::USER1_SUB), KEY_A_PEM, KEY_A_KID);
    let resp = idp.callback_with_token(&login, &token).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert_eq!(location(&resp), "/plan");

    let set_cookie = session_cookie(&resp).expect("session cookie");
    for attr in [
        "Path=/",
        "HttpOnly",
        "SameSite=Lax",
        "Secure",
        "Max-Age=86400",
    ] {
        assert!(set_cookie.contains(attr), "{set_cookie} lacks {attr}");
    }
    assert!(
        set_cookies(&resp)
            .iter()
            .any(|c| c.starts_with("vareni_oidc_state=;") && c.contains("Max-Age=0"))
    );

    let cookie = set_cookie.split(';').next().unwrap().to_owned();
    let resp = me(&idp, &cookie).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"], idp.ctx.user1_id.to_string());
    let csrf = json["csrf_token"].as_str().unwrap().to_owned();

    // RP-initiated logout goes to the IdP with the ID token hint.
    let resp = idp
        .send(
            Request::post("/api/auth/logout")
                .header("Cookie", &cookie)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("csrf_token={csrf}")))
                .unwrap(),
        )
        .await;
    assert!(resp.status().is_redirection());
    let logout_url = location(&resp);
    assert!(
        logout_url.starts_with(&format!("{}/logout?", idp.issuer())),
        "{logout_url}"
    );
    assert!(logout_url.contains("id_token_hint="));
    assert!(logout_url.contains("client_id=vareni"));
    assert!(logout_url.contains("post_logout_redirect_uri="));
    assert_eq!(me(&idp, &cookie).await.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_sanitizes_return_url() {
    let idp = Idp::start().await;
    let login = idp.login("//evil.com").await;
    let token = sign(&idp.claims(&login, common::USER1_SUB), KEY_A_PEM, KEY_A_KID);
    let resp = idp.callback_with_token(&login, &token).await;
    assert_eq!(location(&resp), "/");
    assert!(session_cookie(&resp).is_some());
}

#[tokio::test]
async fn login_without_email_claim_succeeds() {
    let idp = Idp::start().await;
    let login = idp.login("/").await;
    let mut claims = idp.claims(&login, common::USER2_SUB);
    claims.as_object_mut().unwrap().remove("email");
    let resp = idp
        .callback_with_token(&login, &sign(&claims, KEY_A_PEM, KEY_A_KID))
        .await;
    assert!(session_cookie(&resp).is_some(), "{}", location(&resp));
}

#[tokio::test]
async fn login_replaces_previous_session() {
    let idp = Idp::start().await;
    let login = idp.login("/").await;
    let token = sign(&idp.claims(&login, common::USER1_SUB), KEY_A_PEM, KEY_A_KID);
    idp.mount_token(&token).await;
    let resp = idp
        .send(
            Request::get(format!(
                "/api/auth/callback?code=test-code&state={}",
                login.state
            ))
            .header(
                "Cookie",
                format!("{}; {}", login.cookie, idp.ctx.user1_auth.cookie),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert!(session_cookie(&resp).is_some());
    assert_eq!(
        me(&idp, &idp.ctx.user1_auth.cookie).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn state_mismatch_is_rejected() {
    let idp = Idp::start().await;
    let login = idp.login("/").await;
    let resp = idp.callback(&login, "forged-state").await;
    assert_login_error(&resp, "state_expired");
}

#[tokio::test]
async fn replayed_state_is_rejected() {
    let idp = Idp::start().await;
    let login = idp.login("/").await;
    let token = sign(&idp.claims(&login, common::USER1_SUB), KEY_A_PEM, KEY_A_KID);
    let resp = idp.callback_with_token(&login, &token).await;
    assert!(session_cookie(&resp).is_some());

    let resp = idp.callback(&login, &login.state).await;
    assert_login_error(&resp, "state_expired");
}

#[tokio::test]
async fn idp_error_param_is_login_failed() {
    let idp = Idp::start().await;
    let login = idp.login("/").await;
    let resp = idp
        .send(
            Request::get(format!(
                "/api/auth/callback?error=access_denied&state={}",
                login.state
            ))
            .header("Cookie", &login.cookie)
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_login_error(&resp, "login_failed");
}

#[tokio::test]
async fn token_endpoint_error_is_login_failed() {
    let idp = Idp::start().await;
    let login = idp.login("/").await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "invalid_grant",
        })))
        .mount(&idp.server)
        .await;
    let resp = idp.callback(&login, &login.state).await;
    assert_login_error(&resp, "login_failed");
}

async fn assert_token_rejected(modify: impl FnOnce(&mut Value), code: &str) {
    let idp = Idp::start().await;
    let login = idp.login("/").await;
    let mut claims = idp.claims(&login, common::USER1_SUB);
    modify(&mut claims);
    let resp = idp
        .callback_with_token(&login, &sign(&claims, KEY_A_PEM, KEY_A_KID))
        .await;
    assert_login_error(&resp, code);
}

#[tokio::test]
async fn missing_nonce_is_invalid_token() {
    assert_token_rejected(
        |c| {
            c.as_object_mut().unwrap().remove("nonce");
        },
        "invalid_token",
    )
    .await;
}

#[tokio::test]
async fn wrong_nonce_is_invalid_token() {
    assert_token_rejected(|c| c["nonce"] = json!("other-nonce"), "invalid_token").await;
}

#[tokio::test]
async fn wrong_audience_is_invalid_token() {
    assert_token_rejected(|c| c["aud"] = json!("other-client"), "invalid_token").await;
}

#[tokio::test]
async fn wrong_issuer_is_invalid_token() {
    assert_token_rejected(|c| c["iss"] = json!("http://evil.example"), "invalid_token").await;
}

#[tokio::test]
async fn expired_token_is_invalid_token() {
    assert_token_rejected(
        |c| {
            c["exp"] = json!(now() - 600);
            c["iat"] = json!(now() - 900);
        },
        "invalid_token",
    )
    .await;
}

#[tokio::test]
async fn missing_role_is_forbidden_role() {
    assert_token_rejected(|c| c["roles"] = json!(["offline_access"]), "forbidden_role").await;
}

#[tokio::test]
async fn role_substring_is_forbidden_role() {
    assert_token_rejected(|c| c["roles"] = json!(["vareni-admin"]), "forbidden_role").await;
}

#[tokio::test]
async fn unknown_subject_is_forbidden_unknown_user() {
    assert_token_rejected(|c| c["sub"] = json!("sub-nobody"), "forbidden_unknown_user").await;
}

#[tokio::test]
async fn token_signed_by_unknown_key_is_invalid_token() {
    let idp = Idp::start_with_jwks(jwks(&[(KEY_A_KID, KEY_A_N)])).await;
    let login = idp.login("/").await;
    // Claims key A's kid but is signed by key B.
    let token = sign(&idp.claims(&login, common::USER1_SUB), KEY_B_PEM, KEY_A_KID);
    let resp = idp.callback_with_token(&login, &token).await;
    assert_login_error(&resp, "invalid_token");
}

#[tokio::test]
async fn hs256_token_is_invalid_token() {
    let idp = Idp::start().await;
    let login = idp.login("/").await;
    let token = jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &idp.claims(&login, common::USER1_SUB),
        &EncodingKey::from_secret(b"test-client-secret"),
    )
    .unwrap();
    let resp = idp.callback_with_token(&login, &token).await;
    assert_login_error(&resp, "invalid_token");
}

#[tokio::test]
async fn jwks_rotation_is_picked_up() {
    let idp = Idp::start().await;
    let login = idp.login("/").await;
    let token = sign(&idp.claims(&login, common::USER1_SUB), KEY_B_PEM, KEY_B_KID);
    let resp = idp.callback_with_token(&login, &token).await;
    assert_eq!(location(&resp), "/");
    assert!(session_cookie(&resp).is_some());

    let jwks_fetches = idp
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/jwks")
        .count();
    assert_eq!(jwks_fetches, 2, "discovery + one refresh");
}

#[tokio::test]
async fn dotted_role_claim_path() {
    let server_idp = Idp::start().await;
    let issuer = server_idp.issuer();
    let ctx = common::TestContext::with_config(|c| {
        let oidc = c.oidc.as_mut().unwrap();
        oidc.issuer_url = issuer;
        oidc.role_claim = "realm_access.roles".into();
    })
    .await;
    let idp = Idp {
        server: server_idp.server,
        ctx,
    };
    let login = idp.login("/").await;
    let mut claims = idp.claims(&login, common::USER1_SUB);
    claims["realm_access"] = json!({ "roles": ["vareni"] });
    claims.as_object_mut().unwrap().remove("roles");
    let resp = idp
        .callback_with_token(&login, &sign(&claims, KEY_A_PEM, KEY_A_KID))
        .await;
    assert!(session_cookie(&resp).is_some(), "{}", location(&resp));
}
