# Cooking App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a self-hosted, two-user cooking assistant app with recipe management, AI-powered ingestion/chat, meal planning, and push notifications.

**Architecture:** Rust/Axum backend serving a Vue 3 SPA. PostgreSQL 18 with pgvector for storage. Claude API for recipe parsing, chat editing, and meal plan suggestions. All API endpoints covered by integration tests using testcontainers. Single Docker container for deployment.

**Tech Stack:** Rust 1.90 (edition 2024), Axum 0.8, sqlx 0.8, Vue 3, Vite, Tailwind CSS 4, Pinia, PostgreSQL 18 + pgvector, Anthropic Claude API, testcontainers

**Spec:** See `SPEC.md` in the project root for full details.

---

## File Structure

### Backend

```
backend/
├── Cargo.toml
├── src/
│   ├── main.rs                    # server startup, router assembly, static files
│   ├── config.rs                  # env var loading into Config struct
│   ├── error.rs                   # AppError enum, IntoResponse, From impls
│   ├── auth.rs                    # JWT encode/decode, bcrypt, AuthUser extractor
│   ├── routes/
│   │   ├── mod.rs                 # re-exports all route modules
│   │   ├── auth.rs                # POST login, GET me
│   │   ├── recipes.rs             # CRUD + share/unshare
│   │   ├── ingest.rs              # multipart upload → AI parse → preview
│   │   ├── plan.rs                # meal plan CRUD + AI suggest
│   │   ├── chat.rs                # SSE streaming recipe chat
│   │   ├── push.rs                # subscribe/unsubscribe
│   │   ├── public.rs              # public recipe by slug (no auth)
│   │   └── settings.rs            # dietary restrictions CRUD
│   ├── models.rs                  # all DB model structs (flat file)
│   ├── db/
│   │   ├── mod.rs
│   │   ├── users.rs               # user queries + dietary restrictions
│   │   ├── recipes.rs             # recipe + ingredients + steps + tags queries
│   │   ├── meal_plan.rs           # meal plan entry queries
│   │   └── push.rs                # push subscription queries
│   ├── ai/
│   │   ├── mod.rs
│   │   ├── client.rs              # AnthropicClient: complete() + stream()
│   │   ├── ingest.rs              # ingestion prompt + response parsing
│   │   ├── plan.rs                # meal plan suggestion prompt
│   │   └── chat.rs                # chat system prompt, tool defs, SSE relay
│   └── push.rs                    # background notifier task
├── migrations/
│   └── 001_initial.sql            # full schema
└── tests/
    ├── common/
    │   └── mod.rs                 # TestContext: testcontainers + seed + helpers
    ├── auth_test.rs
    ├── recipes_test.rs
    ├── meal_plan_test.rs
    ├── public_test.rs
    ├── push_test.rs
    └── settings_test.rs
```

### Frontend

```
frontend/
├── package.json
├── vite.config.ts
├── index.html
├── src/
│   ├── main.ts
│   ├── App.vue
│   ├── router.ts
│   ├── style.css                  # Tailwind v4 imports
│   ├── api/
│   │   ├── client.ts              # fetch wrapper with JWT header
│   │   ├── auth.ts                # login, me
│   │   ├── recipes.ts             # recipe CRUD + ingest + share
│   │   ├── plan.ts                # meal plan CRUD + suggest
│   │   └── push.ts                # subscribe/unsubscribe
│   ├── stores/
│   │   ├── auth.ts                # user state, token, login/logout
│   │   ├── recipes.ts             # recipe list, current recipe
│   │   └── plan.ts                # meal plan entries
│   ├── pages/
│   │   ├── LoginPage.vue
│   │   ├── RecipeListPage.vue
│   │   ├── RecipeNewPage.vue
│   │   ├── RecipeDetailPage.vue
│   │   ├── PlanPage.vue
│   │   ├── LogPage.vue
│   │   ├── SettingsPage.vue
│   │   └── PublicRecipePage.vue
│   └── components/
│       ├── RecipeCard.vue
│       ├── RecipeForm.vue
│       ├── CookingMode.vue
│       ├── ChatOverlay.vue
│       ├── PlanCalendar.vue
│       ├── TagChips.vue
│       └── MealSlot.vue
└── public/
    └── sw.js                      # service worker for push notifications
```

---

## Phase 1: Backend Foundation

### Task 1: Project Scaffolding

**Files:**
- Create: `backend/Cargo.toml`
- Create: `docker-compose.yml`
- Create: `.env.example`
- Create: `.gitignore`
- Create: `backend/src/main.rs` (placeholder)

- [ ] **Step 1: Create docker-compose.yml**

```yaml
services:
  postgres:
    image: pgvector/pgvector:pg18
    environment:
      POSTGRES_USER: cooking
      POSTGRES_PASSWORD: cooking
      POSTGRES_DB: cookingapp
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data

volumes:
  pgdata:
```

- [ ] **Step 2: Create backend/Cargo.toml**

```toml
[package]
name = "cooking-app"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = { version = "0.8", features = ["multipart"] }
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.8", features = ["postgres", "runtime-tokio", "uuid", "time", "json"] }
reqwest = { version = "0.12", features = ["json", "stream", "multipart"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
time = { version = "0.3", features = ["serde", "formatting", "parsing"] }
bcrypt = "0.17"
jsonwebtoken = "9"
tower-http = { version = "0.6", features = ["fs", "cors"] }
tower = "0.5"
anyhow = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"
tokio-stream = "0.1"
futures = "0.3"
base64 = "0.22"
scraper = "0.22"
web-push = "0.10"
rand = "0.9"

[dev-dependencies]
testcontainers = "0.24"
testcontainers-modules = { version = "0.12", features = ["postgres"] }
tower = { version = "0.5", features = ["util"] }
http-body-util = "0.1"
mime = "0.3"
```

- [ ] **Step 3: Create .env.example**

```env
DATABASE_URL=postgresql://cooking:cooking@localhost:5432/cookingapp
ANTHROPIC_API_KEY=sk-ant-...
JWT_SECRET=change-me-to-a-random-string
JWT_EXPIRY_HOURS=720
VAPID_PUBLIC_KEY=
VAPID_PRIVATE_KEY=
VAPID_CONTACT=mailto:you@example.com
BASE_URL=http://localhost:8080
PUSH_NOTIFY_HOUR=20
STATIC_DIR=./static
UPLOAD_DIR=./uploads
```

- [ ] **Step 4: Create .gitignore**

```gitignore
/target/
/backend/target/
/frontend/node_modules/
/frontend/dist/
.env
*.swp
*.swo
/uploads/
```

- [ ] **Step 5: Create placeholder main.rs**

```rust
fn main() {
    println!("Hello, cooking app!");
}
```

- [ ] **Step 6: Verify project compiles**

Run: `cd backend && cargo check`
Expected: compiles (may take a while for first dependency download)

- [ ] **Step 7: Start Postgres and verify connection**

Run: `docker compose up -d && sleep 2 && docker compose exec postgres psql -U cooking -d cookingapp -c "SELECT 1"`
Expected: returns `1`

- [ ] **Step 8: Commit**

```bash
git init
git add -A
git commit -m "feat: project scaffolding — Cargo.toml, docker-compose, .env"
```

---

### Task 2: Database Migrations

**Files:**
- Create: `backend/migrations/001_initial.sql`

- [ ] **Step 1: Create the migration file**

```sql
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
CREATE EXTENSION IF NOT EXISTS "vector";

CREATE TABLE users (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name TEXT NOT NULL,
  email TEXT UNIQUE NOT NULL,
  password_hash TEXT NOT NULL,
  created_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE user_dietary_restrictions (
  user_id UUID REFERENCES users(id) ON DELETE CASCADE,
  restriction TEXT NOT NULL,
  PRIMARY KEY (user_id, restriction)
);

CREATE TABLE ingredients (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name TEXT UNIQUE NOT NULL,
  unit_default TEXT
);

CREATE TABLE recipes (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  owner_id UUID REFERENCES users(id),
  title TEXT NOT NULL,
  description TEXT,
  servings INTEGER,
  prep_time_min INTEGER,
  cook_time_min INTEGER,
  source_type TEXT CHECK (source_type IN ('manual', 'photo', 'url')),
  source_url TEXT,
  cover_image_path TEXT,
  is_public BOOLEAN DEFAULT false,
  public_slug TEXT UNIQUE,
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE recipe_tags (
  recipe_id UUID REFERENCES recipes(id) ON DELETE CASCADE,
  tag TEXT NOT NULL,
  PRIMARY KEY (recipe_id, tag)
);

CREATE TABLE recipe_ingredients (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  recipe_id UUID REFERENCES recipes(id) ON DELETE CASCADE,
  ingredient_id UUID REFERENCES ingredients(id),
  amount NUMERIC,
  unit TEXT,
  note TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_recipe_ingredients_recipe ON recipe_ingredients(recipe_id);

CREATE TABLE recipe_steps (
  recipe_id UUID REFERENCES recipes(id) ON DELETE CASCADE,
  step_order INTEGER NOT NULL,
  instruction TEXT NOT NULL,
  PRIMARY KEY (recipe_id, step_order)
);

CREATE TABLE meal_plan_entries (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID REFERENCES users(id),
  date DATE NOT NULL,
  meal_type TEXT CHECK (meal_type IN ('breakfast', 'lunch', 'dinner', 'snack')),
  recipe_id UUID REFERENCES recipes(id) ON DELETE SET NULL,
  free_text TEXT,
  servings INTEGER,
  status TEXT CHECK (status IN ('suggested', 'confirmed', 'cooked')) DEFAULT 'confirmed',
  entry_type TEXT CHECK (entry_type IN ('planned', 'logged')) DEFAULT 'logged',
  suggested_by_ai BOOLEAN DEFAULT false,
  note TEXT,
  created_at TIMESTAMPTZ DEFAULT now(),
  CONSTRAINT recipe_or_freetext CHECK (recipe_id IS NOT NULL OR free_text IS NOT NULL)
);

CREATE TABLE recipe_edit_sessions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  recipe_id UUID REFERENCES recipes(id) ON DELETE CASCADE,
  user_id UUID REFERENCES users(id),
  messages JSONB NOT NULL DEFAULT '[]',
  created_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE push_subscriptions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID REFERENCES users(id) ON DELETE CASCADE,
  subscription JSONB NOT NULL,
  created_at TIMESTAMPTZ DEFAULT now()
);
```

- [ ] **Step 2: Run migration**

Run: `cd backend && cargo sqlx migrate run`
(Requires `sqlx-cli`: `cargo install sqlx-cli --no-default-features --features postgres`)
Expected: migration applied successfully

- [ ] **Step 3: Verify tables exist**

Run: `docker compose exec postgres psql -U cooking -d cookingapp -c "\dt"`
Expected: lists all 9 tables

- [ ] **Step 4: Commit**

```bash
git add backend/migrations/
git commit -m "feat: database schema — all tables, indexes, constraints"
```

---

### Task 3: Config Module

**Files:**
- Create: `backend/src/config.rs`

- [ ] **Step 1: Write config.rs**

```rust
use std::env;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub anthropic_api_key: String,
    pub jwt_secret: String,
    pub jwt_expiry_hours: i64,
    pub base_url: String,
    pub push_notify_hour: u32,
    pub static_dir: String,
    pub upload_dir: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: env::var("DATABASE_URL")?,
            anthropic_api_key: env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
            jwt_secret: env::var("JWT_SECRET")?,
            jwt_expiry_hours: env::var("JWT_EXPIRY_HOURS")
                .unwrap_or_else(|_| "720".into())
                .parse()?,
            base_url: env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:8080".into()),
            push_notify_hour: env::var("PUSH_NOTIFY_HOUR")
                .unwrap_or_else(|_| "20".into())
                .parse()?,
            static_dir: env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into()),
            upload_dir: env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".into()),
        })
    }
}
```

- [ ] **Step 2: Verify it compiles**

Add `mod config;` to `main.rs` temporarily. Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add backend/src/config.rs
git commit -m "feat: config module — env var loading"
```

---

### Task 4: Error Handling

**Files:**
- Create: `backend/src/error.rs`

- [ ] **Step 1: Write error.rs**

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found")]
    NotFound,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::Internal(err) => {
                tracing::error!("Internal error: {err:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".into())
            }
            AppError::Sqlx(err) => {
                tracing::error!("Database error: {err}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".into())
            }
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
```

- [ ] **Step 2: Verify compilation**

Add `mod error;` to `main.rs`. Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add backend/src/error.rs
git commit -m "feat: AppError type with IntoResponse"
```

---

### Task 5: Models

**Files:**
- Create: `backend/src/models.rs`

- [ ] **Step 1: Write all model structs**

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

// ── Users ──

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub created_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize)]
pub struct UserWithRestrictions {
    #[serde(flatten)]
    pub user: User,
    pub dietary_restrictions: Vec<String>,
}

// ── Auth ──

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: User,
}

// ── Recipes ──

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Recipe {
    pub id: Uuid,
    pub owner_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub servings: Option<i32>,
    pub prep_time_min: Option<i32>,
    pub cook_time_min: Option<i32>,
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub cover_image_path: Option<String>,
    pub is_public: Option<bool>,
    pub public_slug: Option<String>,
    pub created_at: Option<OffsetDateTime>,
    pub updated_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize)]
pub struct RecipeDetail {
    #[serde(flatten)]
    pub recipe: Recipe,
    pub ingredients: Vec<RecipeIngredient>,
    pub steps: Vec<RecipeStep>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct RecipeIngredient {
    pub id: Uuid,
    pub recipe_id: Uuid,
    pub ingredient_id: Option<Uuid>,
    pub name: String, // joined from ingredients table
    pub amount: Option<sqlx::types::BigDecimal>,
    pub unit: Option<String>,
    pub note: Option<String>,
    pub sort_order: i32,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct RecipeStep {
    pub recipe_id: Uuid,
    pub step_order: i32,
    pub instruction: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRecipeRequest {
    pub title: String,
    pub description: Option<String>,
    pub servings: Option<i32>,
    pub prep_time_min: Option<i32>,
    pub cook_time_min: Option<i32>,
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub tags: Option<Vec<String>>,
    pub ingredients: Vec<IngredientInput>,
    pub steps: Vec<StepInput>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IngredientInput {
    pub name: String,
    pub amount: Option<f64>,
    pub unit: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StepInput {
    pub step_order: i32,
    pub instruction: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRecipeRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub servings: Option<i32>,
    pub prep_time_min: Option<i32>,
    pub cook_time_min: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub ingredients: Option<Vec<IngredientInput>>,
    pub steps: Option<Vec<StepInput>>,
}

#[derive(Debug, Deserialize)]
pub struct RecipeListQuery {
    pub q: Option<String>,
    pub tag: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

// ── Meal Plan ──

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct MealPlanEntry {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub date: time::Date,
    pub meal_type: Option<String>,
    pub recipe_id: Option<Uuid>,
    pub free_text: Option<String>,
    pub servings: Option<i32>,
    pub status: Option<String>,
    pub entry_type: Option<String>,
    pub suggested_by_ai: Option<bool>,
    pub note: Option<String>,
    pub created_at: Option<OffsetDateTime>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMealPlanRequest {
    pub date: String, // YYYY-MM-DD
    pub meal_type: String,
    pub recipe_id: Option<Uuid>,
    pub free_text: Option<String>,
    pub servings: Option<i32>,
    pub status: Option<String>,
    pub entry_type: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMealPlanRequest {
    pub date: Option<String>,
    pub meal_type: Option<String>,
    pub recipe_id: Option<Uuid>,
    pub free_text: Option<String>,
    pub servings: Option<i32>,
    pub status: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MealPlanQuery {
    pub from: String, // YYYY-MM-DD
    pub to: String,   // YYYY-MM-DD
}

#[derive(Debug, Deserialize)]
pub struct MealPlanHistoryQuery {
    pub days: Option<i64>,
}

// ── Push ──

#[derive(Debug, Deserialize)]
pub struct PushSubscriptionRequest {
    pub subscription: serde_json::Value,
}

// ── Public ──

#[derive(Debug, Serialize)]
pub struct ShareResponse {
    pub share_url: String,
    pub slug: String,
}

// ── Settings ──

#[derive(Debug, Deserialize)]
pub struct DietaryRestrictionRequest {
    pub restriction: String,
}

// ── Pagination ──

#[derive(Debug, Serialize)]
pub struct Paginated<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}
```

- [ ] **Step 2: Verify compilation**

Add `mod models;` to `main.rs`. Run: `cargo check`

Note: `RecipeIngredient` uses a joined `name` field — the DB query will JOIN `ingredients` to get the name. The `BigDecimal` type comes from sqlx's postgres feature for NUMERIC columns.

- [ ] **Step 3: Commit**

```bash
git add backend/src/models.rs
git commit -m "feat: all model structs — users, recipes, meal plan, push"
```

---

### Task 6: Test Infrastructure

**Files:**
- Create: `backend/tests/common/mod.rs`

- [ ] **Step 1: Write TestContext with testcontainers**

```rust
use axum::Router;
use sqlx::PgPool;
use std::sync::Arc;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage};

pub struct TestContext {
    pub pool: PgPool,
    pub router: Router,
    pub user1_token: String,
    pub user1_id: uuid::Uuid,
    pub user2_token: String,
    pub user2_id: uuid::Uuid,
    _container: ContainerAsync<GenericImage>,
}

impl TestContext {
    pub async fn new() -> Self {
        let image = GenericImage::new("pgvector/pgvector", "pg18")
            .with_exposed_port(testcontainers::core::ContainerPort::Tcp(5432))
            .with_env_var("POSTGRES_DB", "test")
            .with_env_var("POSTGRES_USER", "test")
            .with_env_var("POSTGRES_PASSWORD", "test")
            .with_wait_for(testcontainers::core::WaitFor::message_on_stderr(
                "database system is ready to accept connections",
            ));

        let container = image.start().await.expect("Failed to start postgres");
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let database_url = format!("postgresql://test:test@127.0.0.1:{port}/test");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test DB");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        // Seed two test users
        let password_hash =
            bcrypt::hash("testpass123", bcrypt::DEFAULT_COST).expect("Failed to hash");

        let user1 = sqlx::query_as::<_, (uuid::Uuid,)>(
            "INSERT INTO users (name, email, password_hash) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind("Test User 1")
        .bind("user1@test.com")
        .bind(&password_hash)
        .fetch_one(&pool)
        .await
        .expect("Failed to seed user1");

        let user2 = sqlx::query_as::<_, (uuid::Uuid,)>(
            "INSERT INTO users (name, email, password_hash) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind("Test User 2")
        .bind("user2@test.com")
        .bind(&password_hash)
        .fetch_one(&pool)
        .await
        .expect("Failed to seed user2");

        let config = cooking_app::config::Config {
            database_url,
            anthropic_api_key: String::new(),
            jwt_secret: "test-secret-key-for-testing".into(),
            jwt_expiry_hours: 24,
            base_url: "http://localhost:8080".into(),
            push_notify_hour: 20,
            static_dir: "./static".into(),
            upload_dir: "/tmp/cooking-test-uploads".into(),
        };

        let user1_token = cooking_app::auth::encode_jwt(user1.0, &config.jwt_secret, config.jwt_expiry_hours)
            .expect("Failed to encode JWT");
        let user2_token = cooking_app::auth::encode_jwt(user2.0, &config.jwt_secret, config.jwt_expiry_hours)
            .expect("Failed to encode JWT");

        let state = cooking_app::AppState {
            pool: pool.clone(),
            config: Arc::new(config),
        };
        let router = cooking_app::create_router(state);

        Self {
            pool,
            router,
            user1_token,
            user1_id: user1.0,
            user2_token,
            user2_id: user2.0,
            _container: container,
        }
    }

    /// Build a request with auth header for user 1
    pub fn auth_header_1(&self) -> (String, String) {
        ("Authorization".into(), format!("Bearer {}", self.user1_token))
    }

    /// Build a request with auth header for user 2
    pub fn auth_header_2(&self) -> (String, String) {
        ("Authorization".into(), format!("Bearer {}", self.user2_token))
    }
}
```

- [ ] **Step 2: Create a trivial test to verify the infrastructure works**

Create `backend/tests/smoke_test.rs`:

```rust
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn test_infrastructure_works() {
    let ctx = common::TestContext::new().await;

    // Verify we can query the database
    let result = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(result, 1);
}
```

Note: This test won't compile yet — it depends on `AppState` and `create_router` which we'll build in later tasks. That's expected. The test infrastructure is written first so that subsequent tasks can use it immediately.

- [ ] **Step 3: Commit**

```bash
git add backend/tests/
git commit -m "feat: test infrastructure — testcontainers, TestContext, seed users"
```

---

## Phase 2: Auth

### Task 7: Auth Utilities + JWT

**Files:**
- Create: `backend/src/auth.rs`

- [ ] **Step 1: Write auth.rs — JWT encode/decode + AuthUser extractor**

```rust
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub exp: i64,
    pub iat: i64,
}

pub fn encode_jwt(user_id: Uuid, secret: &str, expiry_hours: i64) -> anyhow::Result<String> {
    let now = OffsetDateTime::now_utc();
    let claims = Claims {
        sub: user_id,
        iat: now.unix_timestamp(),
        exp: (now + time::Duration::hours(expiry_hours)).unix_timestamp(),
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(token)
}

pub fn decode_jwt(token: &str, secret: &str) -> AppResult<Claims> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::Unauthorized)?;
    Ok(data.claims)
}

/// Extractor that validates JWT and provides the authenticated user ID.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
}

impl<S> FromRequestParts<S> for AuthUser
where
    AppState: axum::extract::FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let token = extract_bearer_token(&parts.headers)?;
        let claims = decode_jwt(&token, &app_state.config.jwt_secret)?;
        Ok(AuthUser {
            user_id: claims.sub,
        })
    }
}

fn extract_bearer_token(headers: &HeaderMap) -> AppResult<String> {
    let header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let token = header
        .strip_prefix("Bearer ")
        .ok_or(AppError::Unauthorized)?;

    Ok(token.to_string())
}
```

- [ ] **Step 2: Verify compilation**

Update `main.rs` to include `pub mod auth;`. Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add backend/src/auth.rs
git commit -m "feat: JWT encode/decode + AuthUser extractor"
```

---

### Task 8: Auth Routes + AppState + Router

**Files:**
- Create: `backend/src/routes/mod.rs`
- Create: `backend/src/routes/auth.rs`
- Create: `backend/src/db/mod.rs`
- Create: `backend/src/db/users.rs`
- Modify: `backend/src/main.rs`

- [ ] **Step 1: Write db/users.rs**

```rust
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::User;

pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT id, name, email, password_hash, created_at FROM users WHERE email = $1")
        .bind(email)
        .fetch_optional(pool)
        .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT id, name, email, password_hash, created_at FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn get_dietary_restrictions(pool: &PgPool, user_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT restriction FROM user_dietary_restrictions WHERE user_id = $1 ORDER BY restriction",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn add_dietary_restriction(pool: &PgPool, user_id: Uuid, restriction: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO user_dietary_restrictions (user_id, restriction) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(restriction)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_dietary_restriction(pool: &PgPool, user_id: Uuid, restriction: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM user_dietary_restrictions WHERE user_id = $1 AND restriction = $2",
    )
    .bind(user_id)
    .bind(restriction)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
```

- [ ] **Step 2: Write db/mod.rs**

```rust
pub mod users;
```

- [ ] **Step 3: Write routes/auth.rs**

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::auth::{encode_jwt, AuthUser};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::{LoginRequest, LoginResponse, UserWithRestrictions};
use crate::AppState;

pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> AppResult<Json<LoginResponse>> {
    let user = db::users::find_by_email(&state.pool, &body.email)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let valid = bcrypt::verify(&body.password, &user.password_hash)
        .map_err(|e| AppError::Internal(e.into()))?;
    if !valid {
        return Err(AppError::Unauthorized);
    }

    let token = encode_jwt(user.id, &state.config.jwt_secret, state.config.jwt_expiry_hours)
        .map_err(|e| AppError::Internal(e))?;

    Ok(Json(LoginResponse { token, user }))
}

pub async fn me(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<UserWithRestrictions>> {
    let user = db::users::find_by_id(&state.pool, auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let restrictions = db::users::get_dietary_restrictions(&state.pool, auth.user_id).await?;

    Ok(Json(UserWithRestrictions {
        user,
        dietary_restrictions: restrictions,
    }))
}
```

- [ ] **Step 4: Write routes/mod.rs**

```rust
pub mod auth;
```

- [ ] **Step 5: Write main.rs with AppState and create_router**

```rust
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use sqlx::PgPool;
use tracing_subscriber::EnvFilter;

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod models;
pub mod routes;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<config::Config>,
}

impl axum::extract::FromRef<AppState> for AppState {
    fn from_ref(state: &AppState) -> Self {
        state.clone()
    }
}

pub fn create_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/auth/login", post(routes::auth::login))
        .route("/auth/me", get(routes::auth::me));

    Router::new()
        .nest("/api", api)
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = config::Config::from_env()?;
    let pool = PgPool::connect(&config.database_url).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let state = AppState {
        pool,
        config: Arc::new(config),
    };

    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("Listening on 0.0.0.0:8080");
    axum::serve(listener, app).await?;

    Ok(())
}
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check`

- [ ] **Step 7: Commit**

```bash
git add backend/src/
git commit -m "feat: auth routes (login, me), AppState, router"
```

---

### Task 9: Auth Integration Tests

**Files:**
- Create: `backend/tests/auth_test.rs`

- [ ] **Step 1: Write auth integration tests**

```rust
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn login_valid_credentials() {
    let ctx = common::TestContext::new().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "email": "user1@test.com",
                "password": "testpass123"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["token"].is_string());
    assert_eq!(json["user"]["email"], "user1@test.com");
    // password_hash must not be in response
    assert!(json["user"]["password_hash"].is_null());
}

#[tokio::test]
async fn login_wrong_password() {
    let ctx = common::TestContext::new().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "email": "user1@test.com",
                "password": "wrongpassword"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_nonexistent_user() {
    let ctx = common::TestContext::new().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "email": "nobody@test.com",
                "password": "testpass123"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_without_token() {
    let ctx = common::TestContext::new().await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_with_invalid_token() {
    let ctx = common::TestContext::new().await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .header("Authorization", "Bearer invalid-garbage-token")
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_with_valid_token() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["email"], "user1@test.com");
    assert!(json["dietary_restrictions"].is_array());
}
```

- [ ] **Step 2: Run tests**

Run: `cd backend && cargo test --test auth_test -- --nocapture`
Expected: all 5 tests pass

- [ ] **Step 3: Commit**

```bash
git add backend/tests/auth_test.rs
git commit -m "test: auth integration tests — login, JWT validation, me endpoint"
```

---

## Phase 3: Recipe CRUD

### Task 10: Recipe DB Queries

**Files:**
- Create: `backend/src/db/recipes.rs`
- Modify: `backend/src/db/mod.rs`

- [ ] **Step 1: Write db/recipes.rs**

```rust
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{
    CreateRecipeRequest, IngredientInput, Recipe, RecipeDetail, RecipeIngredient, RecipeStep,
    StepInput, UpdateRecipeRequest,
};

pub async fn create(
    pool: &PgPool,
    owner_id: Uuid,
    req: &CreateRecipeRequest,
) -> Result<RecipeDetail, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let recipe = sqlx::query_as::<_, Recipe>(
        "INSERT INTO recipes (owner_id, title, description, servings, prep_time_min, cook_time_min, source_type, source_url)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING *",
    )
    .bind(owner_id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(req.servings)
    .bind(req.prep_time_min)
    .bind(req.cook_time_min)
    .bind(&req.source_type)
    .bind(&req.source_url)
    .fetch_one(&mut *tx)
    .await?;

    let ingredients = insert_ingredients(&mut tx, recipe.id, &req.ingredients).await?;
    let steps = insert_steps(&mut tx, recipe.id, &req.steps).await?;
    let tags = if let Some(ref tag_list) = req.tags {
        insert_tags(&mut tx, recipe.id, tag_list).await?;
        tag_list.clone()
    } else {
        vec![]
    };

    tx.commit().await?;

    Ok(RecipeDetail {
        recipe,
        ingredients,
        steps,
        tags,
    })
}

pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<RecipeDetail>, sqlx::Error> {
    let recipe = sqlx::query_as::<_, Recipe>("SELECT * FROM recipes WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    let Some(recipe) = recipe else {
        return Ok(None);
    };

    let ingredients = get_ingredients(pool, id).await?;
    let steps = get_steps(pool, id).await?;
    let tags = get_tags(pool, id).await?;

    Ok(Some(RecipeDetail {
        recipe,
        ingredients,
        steps,
        tags,
    }))
}

pub async fn list(
    pool: &PgPool,
    q: Option<&str>,
    tag: Option<&str>,
    page: i64,
    per_page: i64,
) -> Result<(Vec<Recipe>, i64), sqlx::Error> {
    let offset = (page - 1) * per_page;

    let (items, total) = if let Some(tag_filter) = tag {
        let items = sqlx::query_as::<_, Recipe>(
            "SELECT r.* FROM recipes r
             JOIN recipe_tags rt ON r.id = rt.recipe_id
             WHERE rt.tag = $1
             ORDER BY r.updated_at DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(tag_filter)
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM recipes r
             JOIN recipe_tags rt ON r.id = rt.recipe_id
             WHERE rt.tag = $1",
        )
        .bind(tag_filter)
        .fetch_one(pool)
        .await?;

        (items, total)
    } else if let Some(search) = q {
        let pattern = format!("%{search}%");
        let items = sqlx::query_as::<_, Recipe>(
            "SELECT * FROM recipes WHERE title ILIKE $1 OR description ILIKE $1
             ORDER BY updated_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(&pattern)
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM recipes WHERE title ILIKE $1 OR description ILIKE $1",
        )
        .bind(&pattern)
        .fetch_one(pool)
        .await?;

        (items, total)
    } else {
        let items = sqlx::query_as::<_, Recipe>(
            "SELECT * FROM recipes ORDER BY updated_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        let total =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM recipes")
                .fetch_one(pool)
                .await?;

        (items, total)
    };

    Ok((items, total))
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    req: &UpdateRecipeRequest,
) -> Result<Option<RecipeDetail>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Update recipe fields (only non-None fields)
    let existing = sqlx::query_as::<_, Recipe>("SELECT * FROM recipes WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

    let Some(_) = existing else {
        return Ok(None);
    };

    sqlx::query(
        "UPDATE recipes SET
            title = COALESCE($2, title),
            description = COALESCE($3, description),
            servings = COALESCE($4, servings),
            prep_time_min = COALESCE($5, prep_time_min),
            cook_time_min = COALESCE($6, cook_time_min),
            updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(req.servings)
    .bind(req.prep_time_min)
    .bind(req.cook_time_min)
    .execute(&mut *tx)
    .await?;

    // Replace ingredients if provided
    if let Some(ref ingredients) = req.ingredients {
        sqlx::query("DELETE FROM recipe_ingredients WHERE recipe_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        insert_ingredients(&mut tx, id, ingredients).await?;
    }

    // Replace steps if provided
    if let Some(ref steps) = req.steps {
        sqlx::query("DELETE FROM recipe_steps WHERE recipe_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        insert_steps(&mut tx, id, steps).await?;
    }

    // Replace tags if provided
    if let Some(ref tags) = req.tags {
        sqlx::query("DELETE FROM recipe_tags WHERE recipe_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        insert_tags(&mut tx, id, tags).await?;
    }

    tx.commit().await?;

    get_by_id(pool, id).await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM recipes WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_public_slug(
    pool: &PgPool,
    id: Uuid,
    slug: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE recipes SET is_public = true, public_slug = $2 WHERE id = $1")
        .bind(id)
        .bind(slug)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn remove_public_slug(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE recipes SET is_public = false, public_slug = NULL WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_by_slug(pool: &PgPool, slug: &str) -> Result<Option<RecipeDetail>, sqlx::Error> {
    let recipe = sqlx::query_as::<_, Recipe>(
        "SELECT * FROM recipes WHERE public_slug = $1 AND is_public = true",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;

    let Some(recipe) = recipe else {
        return Ok(None);
    };

    let ingredients = get_ingredients(pool, recipe.id).await?;
    let steps = get_steps(pool, recipe.id).await?;
    let tags = get_tags(pool, recipe.id).await?;

    Ok(Some(RecipeDetail {
        recipe,
        ingredients,
        steps,
        tags,
    }))
}

// ── Helpers ──

async fn insert_ingredients(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    recipe_id: Uuid,
    ingredients: &[IngredientInput],
) -> Result<Vec<RecipeIngredient>, sqlx::Error> {
    let mut result = Vec::new();
    for (i, ing) in ingredients.iter().enumerate() {
        // Upsert ingredient by name
        let ingredient_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO ingredients (name) VALUES ($1)
             ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
             RETURNING id",
        )
        .bind(&ing.name)
        .fetch_one(&mut **tx)
        .await?;

        let row = sqlx::query_as::<_, RecipeIngredient>(
            "INSERT INTO recipe_ingredients (recipe_id, ingredient_id, amount, unit, note, sort_order)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id, recipe_id, ingredient_id, $7::text AS name, amount, unit, note, sort_order",
        )
        .bind(recipe_id)
        .bind(ingredient_id)
        .bind(ing.amount.map(|a| sqlx::types::BigDecimal::try_from(a).unwrap_or_default()))
        .bind(&ing.unit)
        .bind(&ing.note)
        .bind(i as i32)
        .bind(&ing.name)
        .fetch_one(&mut **tx)
        .await?;

        result.push(row);
    }
    Ok(result)
}

async fn insert_steps(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    recipe_id: Uuid,
    steps: &[StepInput],
) -> Result<Vec<RecipeStep>, sqlx::Error> {
    let mut result = Vec::new();
    for step in steps {
        let row = sqlx::query_as::<_, RecipeStep>(
            "INSERT INTO recipe_steps (recipe_id, step_order, instruction)
             VALUES ($1, $2, $3) RETURNING *",
        )
        .bind(recipe_id)
        .bind(step.step_order)
        .bind(&step.instruction)
        .fetch_one(&mut **tx)
        .await?;
        result.push(row);
    }
    Ok(result)
}

async fn insert_tags(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    recipe_id: Uuid,
    tags: &[String],
) -> Result<(), sqlx::Error> {
    for tag in tags {
        sqlx::query("INSERT INTO recipe_tags (recipe_id, tag) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(recipe_id)
            .bind(tag)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn get_ingredients(pool: &PgPool, recipe_id: Uuid) -> Result<Vec<RecipeIngredient>, sqlx::Error> {
    sqlx::query_as::<_, RecipeIngredient>(
        "SELECT ri.id, ri.recipe_id, ri.ingredient_id, i.name, ri.amount, ri.unit, ri.note, ri.sort_order
         FROM recipe_ingredients ri
         JOIN ingredients i ON ri.ingredient_id = i.id
         WHERE ri.recipe_id = $1
         ORDER BY ri.sort_order",
    )
    .bind(recipe_id)
    .fetch_all(pool)
    .await
}

async fn get_steps(pool: &PgPool, recipe_id: Uuid) -> Result<Vec<RecipeStep>, sqlx::Error> {
    sqlx::query_as::<_, RecipeStep>(
        "SELECT * FROM recipe_steps WHERE recipe_id = $1 ORDER BY step_order",
    )
    .bind(recipe_id)
    .fetch_all(pool)
    .await
}

async fn get_tags(pool: &PgPool, recipe_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT tag FROM recipe_tags WHERE recipe_id = $1 ORDER BY tag",
    )
    .bind(recipe_id)
    .fetch_all(pool)
    .await
}
```

- [ ] **Step 2: Add to db/mod.rs**

```rust
pub mod users;
pub mod recipes;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add backend/src/db/
git commit -m "feat: recipe DB queries — CRUD, ingredients, steps, tags, sharing"
```

---

### Task 11: Recipe Routes

**Files:**
- Create: `backend/src/routes/recipes.rs`
- Create: `backend/src/routes/public.rs`
- Modify: `backend/src/routes/mod.rs`
- Modify: `backend/src/main.rs` (add routes)

- [ ] **Step 1: Write routes/recipes.rs**

```rust
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::{
    CreateRecipeRequest, Paginated, Recipe, RecipeDetail, RecipeListQuery, ShareResponse,
    UpdateRecipeRequest,
};
use crate::AppState;

pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateRecipeRequest>,
) -> AppResult<(StatusCode, Json<RecipeDetail>)> {
    let recipe = db::recipes::create(&state.pool, auth.user_id, &body).await?;
    Ok((StatusCode::CREATED, Json(recipe)))
}

pub async fn list(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(query): Query<RecipeListQuery>,
) -> AppResult<Json<Paginated<Recipe>>> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    let (items, total) =
        db::recipes::list(&state.pool, query.q.as_deref(), query.tag.as_deref(), page, per_page)
            .await?;

    Ok(Json(Paginated {
        items,
        total,
        page,
        per_page,
    }))
}

pub async fn get(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<RecipeDetail>> {
    let recipe = db::recipes::get_by_id(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(recipe))
}

pub async fn update(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRecipeRequest>,
) -> AppResult<Json<RecipeDetail>> {
    let recipe = db::recipes::update(&state.pool, id, &body)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(recipe))
}

pub async fn delete(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let deleted = db::recipes::delete(&state.pool, id).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

pub async fn share(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ShareResponse>> {
    // Check recipe exists
    let detail = db::recipes::get_by_id(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;

    // If already shared, return existing slug
    if let Some(slug) = &detail.recipe.public_slug {
        return Ok(Json(ShareResponse {
            share_url: format!("{}/r/{}", state.config.base_url, slug),
            slug: slug.clone(),
        }));
    }

    // Generate slug: lowercase title + random suffix
    let slug = generate_slug(&detail.recipe.title);
    db::recipes::set_public_slug(&state.pool, id, &slug).await?;

    Ok(Json(ShareResponse {
        share_url: format!("{}/r/{}", state.config.base_url, slug),
        slug,
    }))
}

pub async fn unshare(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    db::recipes::remove_public_slug(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn generate_slug(title: &str) -> String {
    let base: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>();
    let base = base.trim_matches('-').replace("--", "-");
    let suffix: u32 = rand::random::<u32>() % 10000;
    format!("{}-{suffix:04}", &base[..base.len().min(40)])
}
```

- [ ] **Step 2: Write routes/public.rs**

```rust
use axum::extract::{Path, State};
use axum::Json;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::RecipeDetail;
use crate::AppState;

pub async fn get_recipe_by_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Json<RecipeDetail>> {
    let recipe = db::recipes::get_by_slug(&state.pool, &slug)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(recipe))
}
```

- [ ] **Step 3: Update routes/mod.rs**

```rust
pub mod auth;
pub mod public;
pub mod recipes;
```

- [ ] **Step 4: Update create_router in main.rs**

```rust
pub fn create_router(state: AppState) -> Router {
    let api = Router::new()
        // Auth
        .route("/auth/login", post(routes::auth::login))
        .route("/auth/me", get(routes::auth::me))
        // Recipes
        .route("/recipes", get(routes::recipes::list).post(routes::recipes::create))
        .route(
            "/recipes/{id}",
            get(routes::recipes::get)
                .put(routes::recipes::update)
                .delete(routes::recipes::delete),
        )
        .route("/recipes/{id}/share", post(routes::recipes::share).delete(routes::recipes::unshare))
        // Public
        .route("/public/recipes/{slug}", get(routes::public::get_recipe_by_slug));

    Router::new()
        .nest("/api", api)
        .with_state(state)
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check`

- [ ] **Step 6: Commit**

```bash
git add backend/src/routes/ backend/src/main.rs
git commit -m "feat: recipe routes — CRUD, share/unshare, public endpoint"
```

---

### Task 12: Recipe Integration Tests

**Files:**
- Create: `backend/tests/recipes_test.rs`
- Create: `backend/tests/public_test.rs`

- [ ] **Step 1: Write recipe CRUD tests**

```rust
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
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
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 1);

    // Search for something that doesn't exist
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes?q=sushi")
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 1);

    // Non-matching tag
    let req = Request::builder()
        .method("GET")
        .uri("/api/recipes?tag=dessert")
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
```

- [ ] **Step 2: Write public sharing tests**

```rust
// backend/tests/public_test.rs
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
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
                "ingredients": [{ "name": "salt", "amount": 1.0, "unit": "tsp" }],
                "steps": [{ "step_order": 1, "instruction": "Add salt" }]
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
```

- [ ] **Step 3: Run all tests**

Run: `cd backend && cargo test -- --nocapture`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add backend/tests/
git commit -m "test: recipe CRUD + public sharing integration tests"
```

---

## Phase 4: Meal Plan, Push, Settings

### Task 13: Meal Plan DB + Routes

**Files:**
- Create: `backend/src/db/meal_plan.rs`
- Create: `backend/src/routes/plan.rs`
- Modify: `backend/src/db/mod.rs`
- Modify: `backend/src/routes/mod.rs`
- Modify: `backend/src/main.rs`

- [ ] **Step 1: Write db/meal_plan.rs**

```rust
use sqlx::PgPool;
use time::Date;
use time::format_description::well_known::Iso8601;
use uuid::Uuid;

use crate::models::{CreateMealPlanRequest, MealPlanEntry, UpdateMealPlanRequest};

pub fn parse_date(s: &str) -> Result<Date, time::error::Parse> {
    Date::parse(s, &time::format_description::parse("[year]-[month]-[day]").unwrap())
}

pub async fn create(
    pool: &PgPool,
    user_id: Uuid,
    req: &CreateMealPlanRequest,
) -> Result<MealPlanEntry, sqlx::Error> {
    let date = parse_date(&req.date).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

    sqlx::query_as::<_, MealPlanEntry>(
        "INSERT INTO meal_plan_entries (user_id, date, meal_type, recipe_id, free_text, servings, status, entry_type, note)
         VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, 'confirmed'), COALESCE($8, 'logged'), $9)
         RETURNING *",
    )
    .bind(user_id)
    .bind(date)
    .bind(&req.meal_type)
    .bind(req.recipe_id)
    .bind(&req.free_text)
    .bind(req.servings)
    .bind(&req.status)
    .bind(&req.entry_type)
    .bind(&req.note)
    .fetch_one(pool)
    .await
}

pub async fn list_by_range(
    pool: &PgPool,
    from: &str,
    to: &str,
) -> Result<Vec<MealPlanEntry>, sqlx::Error> {
    let from_date = parse_date(from).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    let to_date = parse_date(to).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

    sqlx::query_as::<_, MealPlanEntry>(
        "SELECT * FROM meal_plan_entries WHERE date >= $1 AND date <= $2 ORDER BY date, meal_type",
    )
    .bind(from_date)
    .bind(to_date)
    .fetch_all(pool)
    .await
}

pub async fn history(pool: &PgPool, days: i64) -> Result<Vec<MealPlanEntry>, sqlx::Error> {
    sqlx::query_as::<_, MealPlanEntry>(
        "SELECT * FROM meal_plan_entries WHERE date >= CURRENT_DATE - $1::integer
         ORDER BY date DESC, meal_type",
    )
    .bind(days as i32)
    .fetch_all(pool)
    .await
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    req: &UpdateMealPlanRequest,
) -> Result<Option<MealPlanEntry>, sqlx::Error> {
    let date = req.date.as_ref().map(|d| parse_date(d)).transpose()
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

    let result = sqlx::query_as::<_, MealPlanEntry>(
        "UPDATE meal_plan_entries SET
            date = COALESCE($2, date),
            meal_type = COALESCE($3, meal_type),
            recipe_id = COALESCE($4, recipe_id),
            free_text = COALESCE($5, free_text),
            servings = COALESCE($6, servings),
            status = COALESCE($7, status),
            note = COALESCE($8, note)
         WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(date)
    .bind(&req.meal_type)
    .bind(req.recipe_id)
    .bind(&req.free_text)
    .bind(req.servings)
    .bind(&req.status)
    .bind(&req.note)
    .fetch_optional(pool)
    .await?;

    Ok(result)
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM meal_plan_entries WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
```

- [ ] **Step 2: Write routes/plan.rs**

```rust
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::{
    CreateMealPlanRequest, MealPlanEntry, MealPlanHistoryQuery, MealPlanQuery,
    UpdateMealPlanRequest,
};
use crate::AppState;

pub async fn list(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(query): Query<MealPlanQuery>,
) -> AppResult<Json<Vec<MealPlanEntry>>> {
    let entries = db::meal_plan::list_by_range(&state.pool, &query.from, &query.to).await?;
    Ok(Json(entries))
}

pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateMealPlanRequest>,
) -> AppResult<(StatusCode, Json<MealPlanEntry>)> {
    let entry = db::meal_plan::create(&state.pool, auth.user_id, &body).await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

pub async fn update(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateMealPlanRequest>,
) -> AppResult<Json<MealPlanEntry>> {
    let entry = db::meal_plan::update(&state.pool, id, &body)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(entry))
}

pub async fn delete(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let deleted = db::meal_plan::delete(&state.pool, id).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

pub async fn history(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(query): Query<MealPlanHistoryQuery>,
) -> AppResult<Json<Vec<MealPlanEntry>>> {
    let days = query.days.unwrap_or(90);
    let entries = db::meal_plan::history(&state.pool, days).await?;
    Ok(Json(entries))
}
```

- [ ] **Step 3: Update db/mod.rs and routes/mod.rs**

Add `pub mod meal_plan;` to both.

- [ ] **Step 4: Add meal plan routes to create_router**

```rust
// Inside create_router, add to the api Router:
.route("/plan", get(routes::plan::list).post(routes::plan::create))
.route("/plan/{id}", put(routes::plan::update).delete(routes::plan::delete))
.route("/plan/history", get(routes::plan::history))
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check`

- [ ] **Step 6: Commit**

```bash
git add backend/src/
git commit -m "feat: meal plan CRUD — DB queries + routes"
```

---

### Task 14: Meal Plan Integration Tests

**Files:**
- Create: `backend/tests/meal_plan_test.rs`

- [ ] **Step 1: Write meal plan tests**

```rust
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

async fn create_recipe_for_plan(ctx: &common::TestContext) -> String {
    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/recipes")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "title": "Plan Recipe",
                "ingredients": [{ "name": "salt", "amount": 1.0, "unit": "tsp" }],
                "steps": [{ "step_order": 1, "instruction": "Season" }]
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn create_meal_plan_with_recipe() {
    let ctx = common::TestContext::new().await;
    let recipe_id = create_recipe_for_plan(&ctx).await;

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/plan")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "date": "2026-04-15",
                "meal_type": "dinner",
                "recipe_id": recipe_id,
                "servings": 2
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_meal_plan_with_free_text() {
    let ctx = common::TestContext::new().await;

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/plan")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "date": "2026-04-15",
                "meal_type": "lunch",
                "free_text": "Leftover pizza"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_meal_plan_without_recipe_or_text_fails() {
    let ctx = common::TestContext::new().await;

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/plan")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "date": "2026-04-15",
                "meal_type": "lunch"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    // Should fail due to CHECK constraint
    assert!(resp.status().is_server_error() || resp.status().is_client_error());
}

#[tokio::test]
async fn list_by_date_range() {
    let ctx = common::TestContext::new().await;

    let (key, value) = ctx.auth_header_1();
    // Create two entries on different dates
    for date in ["2026-04-15", "2026-04-20"] {
        let req = Request::builder()
            .method("POST")
            .uri("/api/plan")
            .header("Content-Type", "application/json")
            .header(&key, &value)
            .body(Body::from(
                serde_json::to_string(&json!({
                    "date": date,
                    "meal_type": "dinner",
                    "free_text": "Something"
                }))
                .unwrap(),
            ))
            .unwrap();
        ctx.router.clone().oneshot(req).await.unwrap();
    }

    // Query range that includes only the first
    let req = Request::builder()
        .method("GET")
        .uri("/api/plan?from=2026-04-14&to=2026-04-16")
        .header(&key, &value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn update_meal_plan_status() {
    let ctx = common::TestContext::new().await;

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/plan")
        .header("Content-Type", "application/json")
        .header(&key, &value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "date": "2026-04-15",
                "meal_type": "dinner",
                "free_text": "Soup"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap();

    // Update status to cooked
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/plan/{id}"))
        .header("Content-Type", "application/json")
        .header(&key, &value)
        .body(Body::from(
            serde_json::to_string(&json!({ "status": "cooked" })).unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "cooked");
}

#[tokio::test]
async fn delete_meal_plan_entry() {
    let ctx = common::TestContext::new().await;

    let (key, value) = ctx.auth_header_1();
    let req = Request::builder()
        .method("POST")
        .uri("/api/plan")
        .header("Content-Type", "application/json")
        .header(&key, &value)
        .body(Body::from(
            serde_json::to_string(&json!({
                "date": "2026-04-15",
                "meal_type": "dinner",
                "free_text": "Delete me"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/plan/{id}"))
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}
```

- [ ] **Step 2: Run tests**

Run: `cd backend && cargo test --test meal_plan_test -- --nocapture`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add backend/tests/meal_plan_test.rs
git commit -m "test: meal plan integration tests — CRUD, date range, constraint"
```

---

### Task 15: Push Subscriptions + Settings Routes + Tests

**Files:**
- Create: `backend/src/db/push.rs`
- Create: `backend/src/routes/push.rs`
- Create: `backend/src/routes/settings.rs`
- Create: `backend/tests/push_test.rs`
- Create: `backend/tests/settings_test.rs`
- Modify: `backend/src/db/mod.rs`, `backend/src/routes/mod.rs`, `backend/src/main.rs`

- [ ] **Step 1: Write db/push.rs**

```rust
use sqlx::PgPool;
use uuid::Uuid;

pub async fn subscribe(
    pool: &PgPool,
    user_id: Uuid,
    subscription: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    // Upsert: delete existing + insert (simple for 2-user app)
    sqlx::query(
        "INSERT INTO push_subscriptions (user_id, subscription) VALUES ($1, $2)",
    )
    .bind(user_id)
    .bind(subscription)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn unsubscribe(
    pool: &PgPool,
    user_id: Uuid,
    subscription: &serde_json::Value,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM push_subscriptions WHERE user_id = $1 AND subscription = $2",
    )
    .bind(user_id)
    .bind(subscription)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
```

- [ ] **Step 2: Write routes/push.rs**

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::auth::AuthUser;
use crate::db;
use crate::error::AppResult;
use crate::models::PushSubscriptionRequest;
use crate::AppState;

pub async fn subscribe(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<PushSubscriptionRequest>,
) -> AppResult<StatusCode> {
    db::push::subscribe(&state.pool, auth.user_id, &body.subscription).await?;
    Ok(StatusCode::CREATED)
}

pub async fn unsubscribe(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<PushSubscriptionRequest>,
) -> AppResult<StatusCode> {
    db::push::unsubscribe(&state.pool, auth.user_id, &body.subscription).await?;
    Ok(StatusCode::OK)
}
```

- [ ] **Step 3: Write routes/settings.rs**

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::auth::AuthUser;
use crate::db;
use crate::error::AppResult;
use crate::models::DietaryRestrictionRequest;
use crate::AppState;

pub async fn add_restriction(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<DietaryRestrictionRequest>,
) -> AppResult<StatusCode> {
    db::users::add_dietary_restriction(&state.pool, auth.user_id, &body.restriction).await?;
    Ok(StatusCode::CREATED)
}

pub async fn remove_restriction(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<DietaryRestrictionRequest>,
) -> AppResult<StatusCode> {
    db::users::remove_dietary_restriction(&state.pool, auth.user_id, &body.restriction).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 4: Update mod.rs files and router**

Add `pub mod push;` to both `db/mod.rs` and `routes/mod.rs`. Add `pub mod settings;` to `routes/mod.rs`.

Add to `create_router`:
```rust
// Push
.route("/push/subscribe", post(routes::push::subscribe))
.route("/push/unsubscribe", post(routes::push::unsubscribe))
// Settings
.route("/settings/restrictions", post(routes::settings::add_restriction).delete(routes::settings::remove_restriction))
```

- [ ] **Step 5: Write push tests (backend/tests/push_test.rs)**

```rust
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn subscribe_and_unsubscribe() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    let subscription = json!({
        "endpoint": "https://push.example.com/sub1",
        "keys": { "p256dh": "abc", "auth": "def" }
    });

    // Subscribe
    let req = Request::builder()
        .method("POST")
        .uri("/api/push/subscribe")
        .header("Content-Type", "application/json")
        .header(&key, &value)
        .body(Body::from(
            serde_json::to_string(&json!({ "subscription": subscription })).unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Unsubscribe
    let req = Request::builder()
        .method("POST")
        .uri("/api/push/unsubscribe")
        .header("Content-Type", "application/json")
        .header(key, value)
        .body(Body::from(
            serde_json::to_string(&json!({ "subscription": subscription })).unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn duplicate_subscribe_is_idempotent() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    let body = serde_json::to_string(&json!({
        "subscription": {
            "endpoint": "https://push.example.com/sub2",
            "keys": { "p256dh": "abc", "auth": "def" }
        }
    }))
    .unwrap();

    for _ in 0..2 {
        let req = Request::builder()
            .method("POST")
            .uri("/api/push/subscribe")
            .header("Content-Type", "application/json")
            .header(&key, &value)
            .body(Body::from(body.clone()))
            .unwrap();

        let resp = ctx.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }
}
```

- [ ] **Step 6: Write settings tests (backend/tests/settings_test.rs)**

```rust
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn add_and_remove_dietary_restriction() {
    let ctx = common::TestContext::new().await;
    let (key, value) = ctx.auth_header_1();

    // Add restriction
    let req = Request::builder()
        .method("POST")
        .uri("/api/settings/restrictions")
        .header("Content-Type", "application/json")
        .header(&key, &value)
        .body(Body::from(
            serde_json::to_string(&json!({ "restriction": "vegetarian" })).unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Verify via /me
    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .header(&key, &value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["dietary_restrictions"]
        .as_array()
        .unwrap()
        .contains(&json!("vegetarian")));

    // Remove restriction
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/settings/restrictions")
        .header("Content-Type", "application/json")
        .header(&key, &value)
        .body(Body::from(
            serde_json::to_string(&json!({ "restriction": "vegetarian" })).unwrap(),
        ))
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify it's gone
    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .header(key, value)
        .body(Body::empty())
        .unwrap();

    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["dietary_restrictions"].as_array().unwrap().is_empty());
}
```

- [ ] **Step 7: Run all tests**

Run: `cd backend && cargo test -- --nocapture`
Expected: all tests pass

- [ ] **Step 8: Commit**

```bash
git add backend/
git commit -m "feat: push subscriptions + dietary restrictions + integration tests"
```

---

## Phase 5: AI Integration

### Task 16: Anthropic API Client

**Files:**
- Create: `backend/src/ai/mod.rs`
- Create: `backend/src/ai/client.rs`

- [ ] **Step 1: Write ai/client.rs**

```rust
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

#[derive(Clone)]
pub struct AnthropicClient {
    client: Client,
    api_key: String,
}

#[derive(Debug, Serialize)]
pub struct Message {
    pub role: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct StreamEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub index: Option<u32>,
    #[serde(default)]
    pub delta: Option<serde_json::Value>,
    #[serde(default)]
    pub content_block: Option<serde_json::Value>,
}

impl AnthropicClient {
    pub fn new(api_key: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
        }
    }

    pub async fn complete(
        &self,
        model: &str,
        system: &str,
        messages: Vec<Message>,
        max_tokens: u32,
    ) -> anyhow::Result<String> {
        let body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": messages,
        });

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let json: serde_json::Value = resp.json().await?;

        // Extract text from content blocks
        let text = json["content"]
            .as_array()
            .and_then(|blocks| {
                blocks
                    .iter()
                    .find(|b| b["type"] == "text")
                    .and_then(|b| b["text"].as_str())
            })
            .unwrap_or("")
            .to_string();

        Ok(text)
    }

    pub async fn stream_raw(
        &self,
        model: &str,
        system: &str,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
        max_tokens: u32,
    ) -> anyhow::Result<impl Stream<Item = anyhow::Result<bytes::Bytes>>> {
        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": messages,
            "stream": true,
        });

        if let Some(tools) = tools {
            body["tools"] = serde_json::to_value(tools)?;
        }

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let stream = resp.bytes_stream().map(|result| {
            result.map_err(|e| anyhow::anyhow!(e))
        });

        Ok(stream)
    }
}
```

- [ ] **Step 2: Write ai/mod.rs**

```rust
pub mod client;
pub mod ingest;
pub mod plan;
pub mod chat;
```

Create placeholder files for `ingest.rs`, `plan.rs`, `chat.rs` (empty, just so it compiles):

```rust
// ai/ingest.rs — will be implemented in Task 17
// ai/plan.rs — will be implemented in Task 20
// ai/chat.rs — will be implemented in Task 19
```

- [ ] **Step 3: Add `pub mod ai;` to main.rs, verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add backend/src/ai/
git commit -m "feat: Anthropic API client — complete() + stream_raw()"
```

---

### Task 17: Ingestion Pipeline

**Files:**
- Write: `backend/src/ai/ingest.rs`
- Create: `backend/src/routes/ingest.rs`
- Modify: `backend/src/routes/mod.rs`, `backend/src/main.rs`

- [ ] **Step 1: Write ai/ingest.rs**

```rust
use crate::ai::client::{AnthropicClient, Message};
use crate::models::{IngredientInput, StepInput};
use serde::{Deserialize, Serialize};

const INGEST_MODEL: &str = "claude-haiku-4-5-20251001";
const INGEST_SYSTEM: &str = r#"You are a recipe parser. Extract the recipe from the user's input and return ONLY valid JSON.
No preamble, no markdown, no explanation. Schema:
{
  "title": string,
  "description": string | null,
  "servings": number | null,
  "prep_time_min": number | null,
  "cook_time_min": number | null,
  "tags": [string],
  "ingredients": [{ "name": string, "amount": number | null, "unit": string | null, "note": string | null }],
  "steps": [{ "step_order": number, "instruction": string }]
}

For tags: infer relevant categories from the recipe content. Examples: "quick", "vegetarian",
"vegan", "soup", "salad", "pasta", "Asian", "Czech", "dessert", "breakfast", "one-pot".
Assign 1-5 tags."#;

#[derive(Debug, Serialize, Deserialize)]
pub struct ParsedRecipe {
    pub title: String,
    pub description: Option<String>,
    pub servings: Option<i32>,
    pub prep_time_min: Option<i32>,
    pub cook_time_min: Option<i32>,
    pub tags: Vec<String>,
    pub ingredients: Vec<IngredientInput>,
    pub steps: Vec<StepInput>,
}

pub async fn parse_text(
    client: &AnthropicClient,
    text: &str,
) -> anyhow::Result<ParsedRecipe> {
    let messages = vec![Message {
        role: "user".into(),
        content: serde_json::json!(text),
    }];

    let response = client.complete(INGEST_MODEL, INGEST_SYSTEM, messages, 4096).await?;
    let parsed: ParsedRecipe = serde_json::from_str(&response)?;
    Ok(parsed)
}

pub async fn parse_image(
    client: &AnthropicClient,
    image_data: &[u8],
    media_type: &str,
) -> anyhow::Result<ParsedRecipe> {
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, image_data);

    let messages = vec![Message {
        role: "user".into(),
        content: serde_json::json!([
            {
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": media_type,
                    "data": b64,
                }
            },
            {
                "type": "text",
                "text": "Extract the recipe from this image."
            }
        ]),
    }];

    let response = client.complete(INGEST_MODEL, INGEST_SYSTEM, messages, 4096).await?;
    let parsed: ParsedRecipe = serde_json::from_str(&response)?;
    Ok(parsed)
}

pub async fn parse_url(
    client: &AnthropicClient,
    http_client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<ParsedRecipe> {
    let html = http_client.get(url).send().await?.text().await?;
    let document = scraper::Html::parse_document(&html);

    // Extract readable text: try <article>, then <main>, then <body>
    let text = ["article", "main", "body"]
        .iter()
        .find_map(|tag| {
            let selector = scraper::Selector::parse(tag).ok()?;
            document.select(&selector).next().map(|el| el.text().collect::<Vec<_>>().join(" "))
        })
        .unwrap_or_else(|| document.root_element().text().collect::<Vec<_>>().join(" "));

    // Truncate to ~8000 chars to stay within token limits
    let text = if text.len() > 8000 { &text[..8000] } else { &text };

    parse_text(client, text).await
}
```

- [ ] **Step 2: Write routes/ingest.rs**

```rust
use axum::extract::{Multipart, State};
use axum::Json;

use crate::ai;
use crate::ai::client::AnthropicClient;
use crate::ai::ingest::ParsedRecipe;
use crate::error::{AppError, AppResult};
use crate::auth::AuthUser;
use crate::AppState;

pub async fn ingest(
    State(state): State<AppState>,
    _auth: AuthUser,
    mut multipart: Multipart,
) -> AppResult<Json<ParsedRecipe>> {
    let mut source_type = None;
    let mut text = None;
    let mut image_data = None;
    let mut image_media_type = None;
    let mut url = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::BadRequest(e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "source_type" => source_type = Some(field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?),
            "text" => text = Some(field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?),
            "url" => url = Some(field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?),
            "image" => {
                let content_type = field.content_type().unwrap_or("image/jpeg").to_string();
                let data = field.bytes().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
                image_media_type = Some(content_type);
                image_data = Some(data);
            }
            _ => {}
        }
    }

    let source_type = source_type.ok_or_else(|| AppError::BadRequest("source_type is required".into()))?;
    let ai_client = AnthropicClient::new(&state.config.anthropic_api_key);

    let parsed = match source_type.as_str() {
        "manual" => {
            let text = text.ok_or_else(|| AppError::BadRequest("text is required for manual source".into()))?;
            ai::ingest::parse_text(&ai_client, &text).await
                .map_err(|e| AppError::Internal(e))?
        }
        "photo" => {
            let data = image_data.ok_or_else(|| AppError::BadRequest("image is required for photo source".into()))?;
            let media_type = image_media_type.unwrap_or("image/jpeg".into());
            ai::ingest::parse_image(&ai_client, &data, &media_type).await
                .map_err(|e| AppError::Internal(e))?
        }
        "url" => {
            let url = url.ok_or_else(|| AppError::BadRequest("url is required for url source".into()))?;
            ai::ingest::parse_url(&ai_client, &reqwest::Client::new(), &url).await
                .map_err(|e| AppError::Internal(e))?
        }
        other => return Err(AppError::BadRequest(format!("unknown source_type: {other}"))),
    };

    Ok(Json(parsed))
}
```

- [ ] **Step 3: Add to routes/mod.rs and router**

Add `pub mod ingest;` to `routes/mod.rs`.

Add to `create_router`:
```rust
.route("/ingest", post(routes::ingest::ingest))
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git add backend/src/
git commit -m "feat: ingestion pipeline — manual text, photo, URL parsing via Claude"
```

---

### Task 18: Chat Backend (SSE Streaming + Tool Use)

**Files:**
- Write: `backend/src/ai/chat.rs`
- Create: `backend/src/routes/chat.rs`
- Modify: `backend/src/routes/mod.rs`, `backend/src/main.rs`

- [ ] **Step 1: Write ai/chat.rs**

```rust
use crate::ai::client::Tool;

pub fn update_recipe_tool() -> Tool {
    Tool {
        name: "update_recipe".into(),
        description: "Update fields of the current recipe based on the conversation".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "description": { "type": "string" },
                "servings": { "type": "number" },
                "prep_time_min": { "type": "number" },
                "cook_time_min": { "type": "number" },
                "tags": { "type": "array", "items": { "type": "string" } },
                "ingredients": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "amount": { "type": "number" },
                            "unit": { "type": "string" },
                            "note": { "type": "string" }
                        }
                    }
                },
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "step_order": { "type": "number" },
                            "instruction": { "type": "string" }
                        }
                    }
                }
            }
        }),
    }
}

pub fn system_prompt(recipe_json: &str) -> String {
    format!(
        "You are a cooking assistant helping edit a recipe. The current recipe is:\n\
         <recipe>{recipe_json}</recipe>\n\
         When the user asks to change something, respond conversationally AND call the \
         update_recipe tool with only the fields that changed."
    )
}
```

- [ ] **Step 2: Write routes/chat.rs**

```rust
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::StreamExt;
use serde::Deserialize;
use uuid::Uuid;

use crate::ai;
use crate::ai::client::{AnthropicClient, Message};
use crate::auth::AuthUser;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::AppState;

const CHAT_MODEL: &str = "claude-sonnet-4-6";

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub session_id: Option<String>,
}

pub async fn chat(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(recipe_id): Path<Uuid>,
    Json(body): Json<ChatRequest>,
) -> AppResult<Response> {
    let recipe = db::recipes::get_by_id(&state.pool, recipe_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let recipe_json = serde_json::to_string(&recipe).unwrap_or_default();
    let system = ai::chat::system_prompt(&recipe_json);
    let tools = vec![ai::chat::update_recipe_tool()];

    let messages = vec![Message {
        role: "user".into(),
        content: serde_json::json!(body.message),
    }];

    let ai_client = AnthropicClient::new(&state.config.anthropic_api_key);
    let byte_stream = ai_client
        .stream_raw(CHAT_MODEL, &system, messages, Some(tools), 4096)
        .await
        .map_err(|e| AppError::Internal(e))?;

    let body = Body::from_stream(byte_stream);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(body)
        .unwrap())
}
```

- [ ] **Step 3: Add to routes/mod.rs and router**

Add `pub mod chat;` to `routes/mod.rs`.

Add to `create_router`:
```rust
.route("/chat/{recipe_id}", post(routes::chat::chat))
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git add backend/src/
git commit -m "feat: chat SSE endpoint — streaming Claude response with update_recipe tool"
```

---

### Task 19: AI Meal Plan Suggestions

**Files:**
- Write: `backend/src/ai/plan.rs`
- Modify: `backend/src/routes/plan.rs`

- [ ] **Step 1: Write ai/plan.rs**

```rust
use crate::ai::client::{AnthropicClient, Message};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const PLAN_MODEL: &str = "claude-haiku-4-5-20251001";

#[derive(Debug, Serialize, Deserialize)]
pub struct SuggestedEntry {
    pub date: String,
    pub meal_type: String,
    pub recipe_id: Option<Uuid>,
    pub free_text: Option<String>,
    pub note: Option<String>,
}

pub async fn suggest(
    client: &AnthropicClient,
    history_json: &str,
    restrictions_json: &str,
    recipes_json: &str,
    prompt: &str,
) -> anyhow::Result<Vec<SuggestedEntry>> {
    let system = format!(
        "You are a meal planning assistant. Suggest meals for the upcoming days.\n\
         Avoid repeating meals from recent history. Respect all dietary restrictions.\n\
         Prefer variety in tags — don't suggest three soups in a row.\n\n\
         Recent history (last 90 days):\n<history>{history_json}</history>\n\n\
         Dietary restrictions:\n<restrictions>{restrictions_json}</restrictions>\n\n\
         Available recipes (with tags):\n<recipes>{recipes_json}</recipes>\n\n\
         User request: {prompt}\n\n\
         Return ONLY a valid JSON array:\n\
         [{{\"date\": \"YYYY-MM-DD\", \"meal_type\": \"lunch|dinner\", \
         \"recipe_id\": \"uuid or null\", \"free_text\": \"string or null\", \
         \"note\": \"string or null\"}}]"
    );

    let messages = vec![Message {
        role: "user".into(),
        content: serde_json::json!(prompt),
    }];

    let response = client.complete(PLAN_MODEL, &system, messages, 4096).await?;
    let suggestions: Vec<SuggestedEntry> = serde_json::from_str(&response)?;
    Ok(suggestions)
}
```

- [ ] **Step 2: Add suggest route to routes/plan.rs**

```rust
// Add to existing routes/plan.rs:

use crate::ai;
use crate::ai::client::AnthropicClient;

#[derive(Debug, serde::Deserialize)]
pub struct SuggestRequest {
    pub prompt: String,
}

pub async fn suggest(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<SuggestRequest>,
) -> AppResult<Json<Vec<ai::plan::SuggestedEntry>>> {
    let history = db::meal_plan::history(&state.pool, 90).await?;
    let restrictions = db::users::get_dietary_restrictions(&state.pool, auth.user_id).await?;
    let (recipes, _) = db::recipes::list(&state.pool, None, None, 1, 1000).await?;

    let history_json = serde_json::to_string(&history).unwrap_or_default();
    let restrictions_json = serde_json::to_string(&restrictions).unwrap_or_default();
    let recipes_json = serde_json::to_string(&recipes).unwrap_or_default();

    let ai_client = AnthropicClient::new(&state.config.anthropic_api_key);
    let suggestions = ai::plan::suggest(
        &ai_client,
        &history_json,
        &restrictions_json,
        &recipes_json,
        &body.prompt,
    )
    .await
    .map_err(|e| AppError::Internal(e))?;

    Ok(Json(suggestions))
}
```

- [ ] **Step 3: Add suggest route to router**

```rust
.route("/plan/suggest", post(routes::plan::suggest))
```

Note: this route must be registered BEFORE `/plan/{id}` to avoid path conflicts.

- [ ] **Step 4: Verify compilation**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git add backend/src/
git commit -m "feat: AI meal plan suggestions via Claude Haiku"
```

---

### Task 20: Push Notification Background Task

**Files:**
- Create: `backend/src/push.rs`
- Modify: `backend/src/main.rs`

- [ ] **Step 1: Write push.rs — background notifier**

```rust
use sqlx::PgPool;
use time::OffsetDateTime;
use tracing;

pub async fn start_notifier(pool: PgPool, notify_hour: u32) {
    tokio::spawn(async move {
        loop {
            let now = OffsetDateTime::now_utc();
            let current_hour = now.hour() as u32;

            if current_hour == notify_hour {
                if let Err(e) = check_and_notify(&pool).await {
                    tracing::error!("Push notification error: {e}");
                }
                // Sleep until next day (roughly)
                tokio::time::sleep(std::time::Duration::from_secs(23 * 3600)).await;
            } else {
                // Sleep 30 minutes and check again
                tokio::time::sleep(std::time::Duration::from_secs(1800)).await;
            }
        }
    });
}

async fn check_and_notify(pool: &PgPool) -> anyhow::Result<()> {
    // Find users with push subscriptions who haven't logged dinner today
    let rows = sqlx::query_as::<_, (uuid::Uuid, serde_json::Value)>(
        "SELECT ps.user_id, ps.subscription FROM push_subscriptions ps
         WHERE ps.user_id NOT IN (
             SELECT user_id FROM meal_plan_entries
             WHERE date = CURRENT_DATE AND meal_type = 'dinner'
         )",
    )
    .fetch_all(pool)
    .await?;

    for (_user_id, _subscription) in rows {
        // TODO: send web push notification via web-push crate
        // This requires VAPID keys which will be configured in production
        tracing::info!("Would send dinner reminder push notification");
    }

    Ok(())
}
```

- [ ] **Step 2: Start notifier in main.rs**

Add after `sqlx::migrate!()`:
```rust
crate::push::start_notifier(pool.clone(), config.push_notify_hour);
```

Add `pub mod push;` to main.rs module declarations.

- [ ] **Step 3: Verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add backend/src/push.rs backend/src/main.rs
git commit -m "feat: push notification background task — dinner reminder"
```

---

## Phase 6: Frontend Foundation

### Task 21: Vue + Vite + Tailwind Scaffold

**Files:**
- Create: `frontend/package.json`
- Create: `frontend/vite.config.ts`
- Create: `frontend/tsconfig.json`
- Create: `frontend/index.html`
- Create: `frontend/src/main.ts`
- Create: `frontend/src/App.vue`
- Create: `frontend/src/style.css`
- Create: `frontend/src/router.ts`

- [ ] **Step 1: Initialize frontend project**

Run:
```bash
cd frontend && npm create vite@latest . -- --template vue-ts
npm install vue-router@4 pinia @tailwindcss/vite tailwindcss
```

- [ ] **Step 2: Configure vite.config.ts**

```typescript
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  server: {
    proxy: {
      '/api': 'http://localhost:8080',
    },
  },
})
```

- [ ] **Step 3: Create src/style.css**

```css
@import "tailwindcss";
```

- [ ] **Step 4: Create src/router.ts**

```typescript
import { createRouter, createWebHistory } from 'vue-router'

const routes = [
  { path: '/', redirect: '/recipes' },
  { path: '/login', component: () => import('./pages/LoginPage.vue') },
  { path: '/recipes', component: () => import('./pages/RecipeListPage.vue') },
  { path: '/recipes/new', component: () => import('./pages/RecipeNewPage.vue') },
  { path: '/recipes/:id', component: () => import('./pages/RecipeDetailPage.vue') },
  { path: '/plan', component: () => import('./pages/PlanPage.vue') },
  { path: '/log', component: () => import('./pages/LogPage.vue') },
  { path: '/settings', component: () => import('./pages/SettingsPage.vue') },
  { path: '/r/:slug', component: () => import('./pages/PublicRecipePage.vue') },
]

export const router = createRouter({
  history: createWebHistory(),
  routes,
})

// Navigation guard: redirect to login if not authenticated
router.beforeEach((to) => {
  const publicPaths = ['/login', '/r/']
  const isPublic = publicPaths.some(p => to.path.startsWith(p))
  const token = localStorage.getItem('token')

  if (!isPublic && !token) {
    return '/login'
  }
})
```

- [ ] **Step 5: Create src/main.ts**

```typescript
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { router } from './router'
import './style.css'

const app = createApp(App)
app.use(createPinia())
app.use(router)
app.mount('#app')
```

- [ ] **Step 6: Create src/App.vue**

```vue
<template>
  <div class="min-h-screen bg-stone-50">
    <nav v-if="isAuthenticated" class="bg-white border-b border-stone-200 px-4 py-3">
      <div class="max-w-3xl mx-auto flex items-center justify-between">
        <div class="flex gap-6">
          <router-link to="/recipes" class="text-stone-700 font-medium hover:text-orange-600">Recepty</router-link>
          <router-link to="/plan" class="text-stone-700 font-medium hover:text-orange-600">Plan</router-link>
          <router-link to="/log" class="text-stone-700 font-medium hover:text-orange-600">Log</router-link>
        </div>
        <div class="flex items-center gap-4">
          <router-link to="/settings" class="text-stone-500 hover:text-orange-600">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
          </router-link>
          <button @click="logout" class="text-stone-500 text-sm hover:text-red-600">Odhlásit</button>
        </div>
      </div>
    </nav>
    <main class="max-w-3xl mx-auto px-4 py-6">
      <router-view />
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from './stores/auth'

const authStore = useAuthStore()
const router = useRouter()
const isAuthenticated = computed(() => !!authStore.token)

function logout() {
  authStore.logout()
  router.push('/login')
}
</script>
```

- [ ] **Step 7: Create placeholder page components**

Create minimal placeholder files for each page so the router compiles. Each is just:

```vue
<!-- src/pages/LoginPage.vue (and same pattern for all others) -->
<template>
  <div>TODO: LoginPage</div>
</template>
```

Create all: `LoginPage.vue`, `RecipeListPage.vue`, `RecipeNewPage.vue`, `RecipeDetailPage.vue`, `PlanPage.vue`, `LogPage.vue`, `SettingsPage.vue`, `PublicRecipePage.vue`.

- [ ] **Step 8: Verify frontend builds**

Run: `cd frontend && npm run build`
Expected: builds successfully to `dist/`

- [ ] **Step 9: Commit**

```bash
git add frontend/
git commit -m "feat: frontend scaffold — Vue 3, Vite, Tailwind 4, Pinia, router"
```

---

### Task 22: API Client + Auth Store + Login Page

**Files:**
- Create: `frontend/src/api/client.ts`
- Create: `frontend/src/api/auth.ts`
- Create: `frontend/src/stores/auth.ts`
- Modify: `frontend/src/pages/LoginPage.vue`

- [ ] **Step 1: Write api/client.ts**

```typescript
const BASE = '/api'

export async function apiFetch<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const token = localStorage.getItem('token')
  const headers: Record<string, string> = {
    ...(options.headers as Record<string, string> || {}),
  }

  if (token) {
    headers['Authorization'] = `Bearer ${token}`
  }

  if (options.body && typeof options.body === 'string') {
    headers['Content-Type'] = 'application/json'
  }

  const resp = await fetch(`${BASE}${path}`, { ...options, headers })

  if (resp.status === 401) {
    localStorage.removeItem('token')
    window.location.href = '/login'
    throw new Error('Unauthorized')
  }

  if (!resp.ok) {
    const err = await resp.json().catch(() => ({ error: resp.statusText }))
    throw new Error(err.error || resp.statusText)
  }

  if (resp.status === 204) return undefined as T
  return resp.json()
}
```

- [ ] **Step 2: Write api/auth.ts**

```typescript
import { apiFetch } from './client'

export interface User {
  id: string
  name: string
  email: string
  dietary_restrictions: string[]
}

export interface LoginResponse {
  token: string
  user: User
}

export function login(email: string, password: string) {
  return apiFetch<LoginResponse>('/auth/login', {
    method: 'POST',
    body: JSON.stringify({ email, password }),
  })
}

export function me() {
  return apiFetch<User>('/auth/me')
}
```

- [ ] **Step 3: Write stores/auth.ts**

```typescript
import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as authApi from '../api/auth'

export const useAuthStore = defineStore('auth', () => {
  const token = ref(localStorage.getItem('token') || '')
  const user = ref<authApi.User | null>(null)

  async function login(email: string, password: string) {
    const resp = await authApi.login(email, password)
    token.value = resp.token
    user.value = resp.user
    localStorage.setItem('token', resp.token)
  }

  async function fetchMe() {
    user.value = await authApi.me()
  }

  function logout() {
    token.value = ''
    user.value = null
    localStorage.removeItem('token')
  }

  return { token, user, login, fetchMe, logout }
})
```

- [ ] **Step 4: Write LoginPage.vue**

```vue
<template>
  <div class="min-h-[80vh] flex items-center justify-center">
    <form @submit.prevent="handleLogin" class="w-full max-w-sm space-y-6">
      <h1 class="text-2xl font-bold text-stone-800 text-center">Prihlaseni</h1>
      <div v-if="error" class="bg-red-50 text-red-700 p-3 rounded-lg text-sm">{{ error }}</div>
      <div>
        <label class="block text-sm font-medium text-stone-600 mb-1">Email</label>
        <input v-model="email" type="email" required
          class="w-full px-4 py-3 border border-stone-300 rounded-lg focus:ring-2 focus:ring-orange-500 focus:border-orange-500 text-lg" />
      </div>
      <div>
        <label class="block text-sm font-medium text-stone-600 mb-1">Heslo</label>
        <input v-model="password" type="password" required
          class="w-full px-4 py-3 border border-stone-300 rounded-lg focus:ring-2 focus:ring-orange-500 focus:border-orange-500 text-lg" />
      </div>
      <button type="submit" :disabled="loading"
        class="w-full py-3 bg-orange-600 text-white font-medium rounded-lg hover:bg-orange-700 disabled:opacity-50 text-lg">
        {{ loading ? 'Prihlasovani...' : 'Prihlasit' }}
      </button>
    </form>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '../stores/auth'

const authStore = useAuthStore()
const router = useRouter()
const email = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)

async function handleLogin() {
  loading.value = true
  error.value = ''
  try {
    await authStore.login(email.value, password.value)
    router.push('/recipes')
  } catch (e: any) {
    error.value = 'Spatny email nebo heslo'
  } finally {
    loading.value = false
  }
}
</script>
```

- [ ] **Step 5: Verify frontend builds**

Run: `cd frontend && npm run build`

- [ ] **Step 6: Commit**

```bash
git add frontend/src/
git commit -m "feat: API client, auth store, login page"
```

---

### Task 23: Recipe API + Store + List Page

**Files:**
- Create: `frontend/src/api/recipes.ts`
- Create: `frontend/src/stores/recipes.ts`
- Create: `frontend/src/components/RecipeCard.vue`
- Create: `frontend/src/components/TagChips.vue`
- Modify: `frontend/src/pages/RecipeListPage.vue`

- [ ] **Step 1: Write api/recipes.ts**

```typescript
import { apiFetch } from './client'

export interface Recipe {
  id: string
  title: string
  description: string | null
  servings: number | null
  prep_time_min: number | null
  cook_time_min: number | null
  tags?: string[]
  ingredients?: Ingredient[]
  steps?: Step[]
  is_public: boolean
  public_slug: string | null
}

export interface Ingredient {
  id: string
  name: string
  amount: number | null
  unit: string | null
  note: string | null
  sort_order: number
}

export interface Step {
  step_order: number
  instruction: string
}

export interface Paginated<T> {
  items: T[]
  total: number
  page: number
  per_page: number
}

export function listRecipes(params: { q?: string; tag?: string; page?: number }) {
  const search = new URLSearchParams()
  if (params.q) search.set('q', params.q)
  if (params.tag) search.set('tag', params.tag)
  if (params.page) search.set('page', String(params.page))
  return apiFetch<Paginated<Recipe>>(`/recipes?${search}`)
}

export function getRecipe(id: string) {
  return apiFetch<Recipe>(`/recipes/${id}`)
}

export function createRecipe(data: any) {
  return apiFetch<Recipe>('/recipes', { method: 'POST', body: JSON.stringify(data) })
}

export function updateRecipe(id: string, data: any) {
  return apiFetch<Recipe>(`/recipes/${id}`, { method: 'PUT', body: JSON.stringify(data) })
}

export function deleteRecipe(id: string) {
  return apiFetch<void>(`/recipes/${id}`, { method: 'DELETE' })
}

export function shareRecipe(id: string) {
  return apiFetch<{ share_url: string; slug: string }>(`/recipes/${id}/share`, { method: 'POST' })
}

export function unshareRecipe(id: string) {
  return apiFetch<void>(`/recipes/${id}/share`, { method: 'DELETE' })
}

export function ingest(formData: FormData) {
  return apiFetch<any>('/ingest', { method: 'POST', body: formData })
}

export function getPublicRecipe(slug: string) {
  return apiFetch<Recipe>(`/public/recipes/${slug}`)
}
```

- [ ] **Step 2: Write stores/recipes.ts**

```typescript
import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as api from '../api/recipes'

export const useRecipeStore = defineStore('recipes', () => {
  const recipes = ref<api.Recipe[]>([])
  const total = ref(0)
  const loading = ref(false)

  async function fetch(params: { q?: string; tag?: string; page?: number } = {}) {
    loading.value = true
    try {
      const result = await api.listRecipes(params)
      recipes.value = result.items
      total.value = result.total
    } finally {
      loading.value = false
    }
  }

  return { recipes, total, loading, fetch }
})
```

- [ ] **Step 3: Write TagChips.vue**

```vue
<template>
  <div class="flex flex-wrap gap-2">
    <span v-for="tag in tags" :key="tag"
      @click="$emit('select', tag)"
      class="px-3 py-1 rounded-full text-sm cursor-pointer"
      :class="selected === tag
        ? 'bg-orange-600 text-white'
        : 'bg-stone-200 text-stone-700 hover:bg-stone-300'">
      {{ tag }}
    </span>
  </div>
</template>

<script setup lang="ts">
defineProps<{ tags: string[]; selected?: string }>()
defineEmits<{ select: [tag: string] }>()
</script>
```

- [ ] **Step 4: Write RecipeCard.vue**

```vue
<template>
  <a :href="`/recipes/${recipe.id}`" @click.prevent="$router.push(`/recipes/${recipe.id}`)"
    class="block bg-white rounded-xl border border-stone-200 p-4 hover:shadow-md transition-shadow">
    <h3 class="font-semibold text-stone-800 text-lg">{{ recipe.title }}</h3>
    <p v-if="recipe.description" class="text-stone-500 text-sm mt-1 line-clamp-2">{{ recipe.description }}</p>
    <div class="flex items-center gap-3 mt-3 text-sm text-stone-500">
      <span v-if="recipe.prep_time_min">{{ recipe.prep_time_min + (recipe.cook_time_min || 0) }} min</span>
      <span v-if="recipe.servings">{{ recipe.servings }} porci</span>
    </div>
  </a>
</template>

<script setup lang="ts">
import type { Recipe } from '../api/recipes'
defineProps<{ recipe: Recipe }>()
</script>
```

- [ ] **Step 5: Write RecipeListPage.vue**

```vue
<template>
  <div>
    <div class="flex items-center justify-between mb-6">
      <h1 class="text-2xl font-bold text-stone-800">Recepty</h1>
      <router-link to="/recipes/new"
        class="px-4 py-2 bg-orange-600 text-white rounded-lg hover:bg-orange-700 font-medium">
        + Novy recept
      </router-link>
    </div>

    <div class="mb-4">
      <input v-model="search" @input="debouncedFetch" placeholder="Hledat recepty..."
        class="w-full px-4 py-3 border border-stone-300 rounded-lg text-lg" />
    </div>

    <div v-if="store.loading" class="text-center text-stone-500 py-8">Nacitam...</div>
    <div v-else-if="store.recipes.length === 0" class="text-center text-stone-400 py-8">Zadne recepty</div>
    <div v-else class="space-y-3">
      <RecipeCard v-for="recipe in store.recipes" :key="recipe.id" :recipe="recipe" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRecipeStore } from '../stores/recipes'
import RecipeCard from '../components/RecipeCard.vue'

const store = useRecipeStore()
const search = ref('')
let debounceTimer: ReturnType<typeof setTimeout>

function debouncedFetch() {
  clearTimeout(debounceTimer)
  debounceTimer = setTimeout(() => {
    store.fetch({ q: search.value || undefined })
  }, 300)
}

onMounted(() => store.fetch())
</script>
```

- [ ] **Step 6: Verify frontend builds**

Run: `cd frontend && npm run build`

- [ ] **Step 7: Commit**

```bash
git add frontend/src/
git commit -m "feat: recipe list page — search, cards, tag chips"
```

---

### Task 24: Recipe Detail + Cooking Mode

**Files:**
- Create: `frontend/src/components/CookingMode.vue`
- Create: `frontend/src/components/IngredientList.vue`
- Modify: `frontend/src/pages/RecipeDetailPage.vue`

- [ ] **Step 1: Write IngredientList.vue**

```vue
<template>
  <ul class="space-y-2">
    <li v-for="ing in ingredients" :key="ing.id" class="flex items-baseline gap-2 py-1">
      <span class="font-medium text-stone-800">{{ ing.name }}</span>
      <span v-if="ing.amount" class="text-stone-600">{{ ing.amount }}{{ ing.unit ? ' ' + ing.unit : '' }}</span>
      <span v-if="ing.note" class="text-stone-400 text-sm italic">{{ ing.note }}</span>
    </li>
  </ul>
</template>

<script setup lang="ts">
import type { Ingredient } from '../api/recipes'
defineProps<{ ingredients: Ingredient[] }>()
</script>
```

- [ ] **Step 2: Write CookingMode.vue**

```vue
<template>
  <div class="fixed inset-0 bg-stone-900 text-white z-50 flex flex-col" @click="next" @touchstart="handleTouch">
    <div class="flex items-center justify-between p-4">
      <span class="text-stone-400">Krok {{ current + 1 }} / {{ steps.length }}</span>
      <button @click.stop="$emit('close')" class="text-stone-400 hover:text-white p-2 text-xl">✕</button>
    </div>
    <div class="flex-1 flex items-center justify-center px-8">
      <p class="text-2xl sm:text-3xl leading-relaxed text-center font-light">
        {{ steps[current]?.instruction }}
      </p>
    </div>
    <div class="flex justify-center gap-2 pb-8">
      <div v-for="(_, i) in steps" :key="i"
        class="w-3 h-3 rounded-full"
        :class="i === current ? 'bg-orange-500' : 'bg-stone-600'" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import type { Step } from '../api/recipes'

const props = defineProps<{ steps: Step[] }>()
defineEmits<{ close: [] }>()

const current = ref(0)
let touchStartX = 0

function next() {
  if (current.value < props.steps.length - 1) current.value++
}

function handleTouch(e: TouchEvent) {
  touchStartX = e.touches[0].clientX
  const handler = (e2: TouchEvent) => {
    const diff = e2.changedTouches[0].clientX - touchStartX
    if (Math.abs(diff) > 50) {
      if (diff > 0 && current.value > 0) current.value--
      else if (diff < 0 && current.value < props.steps.length - 1) current.value++
    }
    document.removeEventListener('touchend', handler)
  }
  document.addEventListener('touchend', handler)
}
</script>
```

- [ ] **Step 3: Write RecipeDetailPage.vue**

```vue
<template>
  <div v-if="recipe">
    <div class="flex items-start justify-between mb-4">
      <h1 class="text-2xl font-bold text-stone-800">{{ recipe.title }}</h1>
      <div class="flex gap-2">
        <button @click="startCooking"
          class="px-4 py-2 bg-orange-600 text-white rounded-lg hover:bg-orange-700">
          Varit
        </button>
        <button @click="handleShare" class="px-4 py-2 border border-stone-300 rounded-lg hover:bg-stone-100">
          Sdilet
        </button>
      </div>
    </div>

    <p v-if="recipe.description" class="text-stone-600 mb-4">{{ recipe.description }}</p>

    <div class="flex gap-4 text-sm text-stone-500 mb-4">
      <span v-if="recipe.prep_time_min">Priprava: {{ recipe.prep_time_min }} min</span>
      <span v-if="recipe.cook_time_min">Vareni: {{ recipe.cook_time_min }} min</span>
      <span v-if="recipe.servings">{{ recipe.servings }} porci</span>
    </div>

    <TagChips v-if="recipe.tags?.length" :tags="recipe.tags" class="mb-6" />

    <section class="mb-8">
      <h2 class="text-lg font-semibold text-stone-700 mb-3">Ingredience</h2>
      <IngredientList :ingredients="recipe.ingredients || []" />
    </section>

    <section class="mb-8">
      <h2 class="text-lg font-semibold text-stone-700 mb-3">Postup</h2>
      <ol class="space-y-4">
        <li v-for="step in recipe.steps" :key="step.step_order" class="flex gap-3">
          <span class="flex-shrink-0 w-7 h-7 rounded-full bg-orange-100 text-orange-700 flex items-center justify-center text-sm font-medium">
            {{ step.step_order }}
          </span>
          <p class="text-stone-700 pt-0.5">{{ step.instruction }}</p>
        </li>
      </ol>
    </section>

    <!-- Chat FAB -->
    <button @click="showChat = true"
      class="fixed bottom-6 right-6 w-14 h-14 bg-orange-600 text-white rounded-full shadow-lg hover:bg-orange-700 flex items-center justify-center text-2xl z-40">
      💬
    </button>

    <CookingMode v-if="cooking" :steps="recipe.steps || []" @close="cooking = false" />
    <ChatOverlay v-if="showChat" :recipe-id="recipe.id" @close="showChat = false" @update="refreshRecipe" />
  </div>
  <div v-else class="text-center text-stone-400 py-8">Nacitam...</div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRoute } from 'vue-router'
import * as api from '../api/recipes'
import type { Recipe } from '../api/recipes'
import TagChips from '../components/TagChips.vue'
import IngredientList from '../components/IngredientList.vue'
import CookingMode from '../components/CookingMode.vue'
import ChatOverlay from '../components/ChatOverlay.vue'

const route = useRoute()
const recipe = ref<Recipe | null>(null)
const cooking = ref(false)
const showChat = ref(false)

async function loadRecipe() {
  recipe.value = await api.getRecipe(route.params.id as string)
}

function startCooking() { cooking.value = true }

async function handleShare() {
  if (!recipe.value) return
  const result = await api.shareRecipe(recipe.value.id)
  await navigator.clipboard.writeText(result.share_url)
  alert('Odkaz zkopirovan!')
}

function refreshRecipe() { loadRecipe() }

onMounted(loadRecipe)
</script>
```

- [ ] **Step 4: Create ChatOverlay.vue placeholder**

```vue
<template>
  <div class="fixed inset-0 bg-black/50 z-50 flex justify-end">
    <div class="w-full sm:w-96 bg-white h-full flex flex-col">
      <div class="flex items-center justify-between p-4 border-b">
        <h3 class="font-semibold">Chat</h3>
        <button @click="$emit('close')" class="text-stone-500 hover:text-stone-700 text-xl">✕</button>
      </div>
      <div class="flex-1 p-4 text-stone-400">Chat bude implementovan v dalsi uloze</div>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{ recipeId: string }>()
defineEmits<{ close: []; update: [] }>()
</script>
```

- [ ] **Step 5: Verify frontend builds**

Run: `cd frontend && npm run build`

- [ ] **Step 6: Commit**

```bash
git add frontend/src/
git commit -m "feat: recipe detail page — ingredients, steps, cooking mode, share"
```

---

### Task 25: Chat Overlay Component

**Files:**
- Modify: `frontend/src/components/ChatOverlay.vue`

- [ ] **Step 1: Implement the full ChatOverlay**

```vue
<template>
  <div class="fixed inset-0 z-50 flex justify-end sm:bg-black/50">
    <div class="w-full sm:w-96 bg-white h-full flex flex-col shadow-xl">
      <div class="flex items-center justify-between p-4 border-b border-stone-200">
        <h3 class="font-semibold text-stone-800">Upravit recept</h3>
        <button @click="$emit('close')" class="p-2 text-stone-400 hover:text-stone-700">✕</button>
      </div>

      <div ref="messagesEl" class="flex-1 overflow-y-auto p-4 space-y-4">
        <div v-for="(msg, i) in messages" :key="i"
          :class="msg.role === 'user' ? 'ml-8' : 'mr-8'">
          <div :class="msg.role === 'user'
            ? 'bg-orange-50 text-stone-800 rounded-2xl rounded-tr-sm px-4 py-3'
            : 'bg-stone-100 text-stone-800 rounded-2xl rounded-tl-sm px-4 py-3'">
            {{ msg.text }}
          </div>
        </div>
        <div v-if="streaming" class="mr-8">
          <div class="bg-stone-100 text-stone-800 rounded-2xl rounded-tl-sm px-4 py-3">
            {{ streamText }}<span class="animate-pulse">|</span>
          </div>
        </div>
      </div>

      <div v-if="hasUpdates" class="px-4 py-2 border-t bg-orange-50">
        <button @click="saveChanges"
          class="w-full py-2 bg-orange-600 text-white rounded-lg hover:bg-orange-700 font-medium">
          Ulozit zmeny
        </button>
      </div>

      <form @submit.prevent="send" class="p-4 border-t border-stone-200">
        <div class="flex gap-2">
          <input v-model="input" placeholder="Napr. pridej vic cesneku..."
            class="flex-1 px-4 py-3 border border-stone-300 rounded-lg text-lg"
            :disabled="streaming" />
          <button type="submit" :disabled="!input.trim() || streaming"
            class="px-4 py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 disabled:opacity-50">
            →
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, nextTick } from 'vue'
import { updateRecipe } from '../api/recipes'

const props = defineProps<{ recipeId: string }>()
const emit = defineEmits<{ close: []; update: [] }>()

const input = ref('')
const messages = ref<{ role: string; text: string }[]>([])
const streaming = ref(false)
const streamText = ref('')
const hasUpdates = ref(false)
const pendingUpdate = ref<any>(null)
const messagesEl = ref<HTMLElement>()

async function send() {
  const text = input.value.trim()
  if (!text) return
  input.value = ''
  messages.value.push({ role: 'user', text })
  streaming.value = true
  streamText.value = ''

  try {
    const token = localStorage.getItem('token')
    const resp = await fetch(`/api/chat/${props.recipeId}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`,
      },
      body: JSON.stringify({ message: text }),
    })

    const reader = resp.body?.getReader()
    const decoder = new TextDecoder()
    let toolJson = ''
    let inToolUse = false

    while (reader) {
      const { done, value } = await reader.read()
      if (done) break

      const chunk = decoder.decode(value, { stream: true })
      const lines = chunk.split('\n')

      for (const line of lines) {
        if (!line.startsWith('data: ')) continue
        const data = line.slice(6)
        if (data === '[DONE]') continue

        try {
          const event = JSON.parse(data)

          if (event.type === 'content_block_start' && event.content_block?.type === 'tool_use') {
            inToolUse = true
            toolJson = ''
          } else if (event.type === 'content_block_delta') {
            if (inToolUse && event.delta?.partial_json) {
              toolJson += event.delta.partial_json
            } else if (event.delta?.text) {
              streamText.value += event.delta.text
            }
          } else if (event.type === 'content_block_stop' && inToolUse) {
            inToolUse = false
            try {
              pendingUpdate.value = JSON.parse(toolJson)
              hasUpdates.value = true
            } catch { /* ignore parse errors */ }
          }
        } catch { /* ignore non-JSON lines */ }
      }
    }

    if (streamText.value) {
      messages.value.push({ role: 'assistant', text: streamText.value })
    }
  } catch (e: any) {
    messages.value.push({ role: 'assistant', text: `Chyba: ${e.message}` })
  } finally {
    streaming.value = false
    streamText.value = ''
    await nextTick()
    messagesEl.value?.scrollTo({ top: messagesEl.value.scrollHeight })
  }
}

async function saveChanges() {
  if (!pendingUpdate.value) return
  await updateRecipe(props.recipeId, pendingUpdate.value)
  pendingUpdate.value = null
  hasUpdates.value = false
  emit('update')
}
</script>
```

- [ ] **Step 2: Verify frontend builds**

Run: `cd frontend && npm run build`

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/ChatOverlay.vue
git commit -m "feat: chat overlay — SSE streaming, tool use parsing, save changes"
```

---

### Task 26: Recipe Ingestion Page

**Files:**
- Create: `frontend/src/components/RecipeForm.vue`
- Modify: `frontend/src/pages/RecipeNewPage.vue`

- [ ] **Step 1: Write RecipeForm.vue (editable preview)**

```vue
<template>
  <form @submit.prevent="$emit('save', form)" class="space-y-6">
    <div>
      <label class="block text-sm font-medium text-stone-600 mb-1">Nazev</label>
      <input v-model="form.title" required class="w-full px-4 py-3 border border-stone-300 rounded-lg text-lg" />
    </div>
    <div>
      <label class="block text-sm font-medium text-stone-600 mb-1">Popis</label>
      <textarea v-model="form.description" rows="2" class="w-full px-4 py-3 border border-stone-300 rounded-lg" />
    </div>
    <div class="grid grid-cols-3 gap-4">
      <div>
        <label class="block text-sm font-medium text-stone-600 mb-1">Porci</label>
        <input v-model.number="form.servings" type="number" class="w-full px-3 py-2 border border-stone-300 rounded-lg" />
      </div>
      <div>
        <label class="block text-sm font-medium text-stone-600 mb-1">Priprava (min)</label>
        <input v-model.number="form.prep_time_min" type="number" class="w-full px-3 py-2 border border-stone-300 rounded-lg" />
      </div>
      <div>
        <label class="block text-sm font-medium text-stone-600 mb-1">Vareni (min)</label>
        <input v-model.number="form.cook_time_min" type="number" class="w-full px-3 py-2 border border-stone-300 rounded-lg" />
      </div>
    </div>

    <div>
      <label class="block text-sm font-medium text-stone-600 mb-1">Tagy</label>
      <input v-model="tagsInput" placeholder="quick, vegetarian, Czech"
        class="w-full px-4 py-2 border border-stone-300 rounded-lg" />
    </div>

    <div>
      <h3 class="text-sm font-medium text-stone-600 mb-2">Ingredience</h3>
      <div v-for="(ing, i) in form.ingredients" :key="i" class="flex gap-2 mb-2">
        <input v-model="ing.name" placeholder="Nazev" class="flex-1 px-3 py-2 border border-stone-300 rounded-lg" />
        <input v-model.number="ing.amount" type="number" step="any" placeholder="Mnozstvi" class="w-24 px-3 py-2 border border-stone-300 rounded-lg" />
        <input v-model="ing.unit" placeholder="Jednotka" class="w-20 px-3 py-2 border border-stone-300 rounded-lg" />
        <button type="button" @click="form.ingredients.splice(i, 1)" class="text-red-400 hover:text-red-600 px-2">✕</button>
      </div>
      <button type="button" @click="addIngredient" class="text-orange-600 text-sm hover:underline">+ Pridat ingredienci</button>
    </div>

    <div>
      <h3 class="text-sm font-medium text-stone-600 mb-2">Postup</h3>
      <div v-for="(step, i) in form.steps" :key="i" class="flex gap-2 mb-2">
        <span class="flex-shrink-0 w-8 h-8 rounded-full bg-stone-200 flex items-center justify-center text-sm">{{ i + 1 }}</span>
        <textarea v-model="step.instruction" rows="2" class="flex-1 px-3 py-2 border border-stone-300 rounded-lg" />
        <button type="button" @click="form.steps.splice(i, 1)" class="text-red-400 hover:text-red-600 px-2">✕</button>
      </div>
      <button type="button" @click="addStep" class="text-orange-600 text-sm hover:underline">+ Pridat krok</button>
    </div>

    <button type="submit" class="w-full py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 font-medium text-lg">
      Ulozit recept
    </button>
  </form>
</template>

<script setup lang="ts">
import { reactive, computed, watch } from 'vue'

const props = defineProps<{ initial?: any }>()
defineEmits<{ save: [data: any] }>()

const form = reactive({
  title: props.initial?.title || '',
  description: props.initial?.description || '',
  servings: props.initial?.servings || null,
  prep_time_min: props.initial?.prep_time_min || null,
  cook_time_min: props.initial?.cook_time_min || null,
  ingredients: props.initial?.ingredients || [],
  steps: props.initial?.steps || [],
  tags: props.initial?.tags || [],
  source_type: props.initial?.source_type || 'manual',
})

const tagsInput = computed({
  get: () => form.tags.join(', '),
  set: (v: string) => { form.tags = v.split(',').map(t => t.trim()).filter(Boolean) },
})

function addIngredient() {
  form.ingredients.push({ name: '', amount: null, unit: '', note: '' })
}
function addStep() {
  form.steps.push({ step_order: form.steps.length + 1, instruction: '' })
}

watch(() => props.initial, (v) => {
  if (v) Object.assign(form, v)
}, { deep: true })
</script>
```

- [ ] **Step 2: Write RecipeNewPage.vue**

```vue
<template>
  <div>
    <h1 class="text-2xl font-bold text-stone-800 mb-6">Novy recept</h1>

    <!-- Source tabs -->
    <div v-if="!preview" class="flex border-b border-stone-200 mb-6">
      <button v-for="tab in tabs" :key="tab.key" @click="activeTab = tab.key"
        class="px-4 py-2 -mb-px font-medium text-sm"
        :class="activeTab === tab.key
          ? 'border-b-2 border-orange-600 text-orange-600'
          : 'text-stone-500 hover:text-stone-700'">
        {{ tab.label }}
      </button>
    </div>

    <!-- Input forms -->
    <div v-if="!preview">
      <div v-if="activeTab === 'manual'" class="space-y-4">
        <textarea v-model="textInput" rows="8" placeholder="Vloz recept jako text..."
          class="w-full px-4 py-3 border border-stone-300 rounded-lg" />
        <button @click="handleIngest" :disabled="loading" class="px-6 py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 disabled:opacity-50">
          {{ loading ? 'Zpracovavam...' : 'Zpracovat' }}
        </button>
      </div>

      <div v-if="activeTab === 'photo'" class="space-y-4">
        <input type="file" accept="image/*" capture="environment" @change="handleFile"
          class="block w-full text-sm text-stone-500 file:mr-4 file:py-2 file:px-4 file:rounded-lg file:border-0 file:bg-orange-50 file:text-orange-700 file:font-medium" />
        <button @click="handleIngest" :disabled="loading || !imageFile" class="px-6 py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 disabled:opacity-50">
          {{ loading ? 'Zpracovavam...' : 'Zpracovat' }}
        </button>
      </div>

      <div v-if="activeTab === 'url'" class="space-y-4">
        <input v-model="urlInput" type="url" placeholder="https://..."
          class="w-full px-4 py-3 border border-stone-300 rounded-lg" />
        <button @click="handleIngest" :disabled="loading" class="px-6 py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 disabled:opacity-50">
          {{ loading ? 'Zpracovavam...' : 'Zpracovat' }}
        </button>
      </div>
    </div>

    <!-- Preview / Edit form -->
    <div v-if="preview">
      <div class="flex items-center justify-between mb-4">
        <h2 class="text-lg font-semibold text-stone-700">Nahled</h2>
        <button @click="preview = null" class="text-stone-500 hover:text-stone-700 text-sm">← Zpet</button>
      </div>
      <RecipeForm :initial="preview" @save="handleSave" />
    </div>

    <div v-if="error" class="mt-4 bg-red-50 text-red-700 p-3 rounded-lg text-sm">{{ error }}</div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { ingest, createRecipe } from '../api/recipes'
import RecipeForm from '../components/RecipeForm.vue'

const router = useRouter()
const activeTab = ref('manual')
const tabs = [
  { key: 'manual', label: 'Napsat' },
  { key: 'photo', label: 'Fotka' },
  { key: 'url', label: 'Web' },
]

const textInput = ref('')
const urlInput = ref('')
const imageFile = ref<File | null>(null)
const preview = ref<any>(null)
const loading = ref(false)
const error = ref('')

function handleFile(e: Event) {
  const input = e.target as HTMLInputElement
  imageFile.value = input.files?.[0] || null
}

async function handleIngest() {
  loading.value = true
  error.value = ''
  try {
    const form = new FormData()
    form.append('source_type', activeTab.value)
    if (activeTab.value === 'manual') form.append('text', textInput.value)
    if (activeTab.value === 'photo' && imageFile.value) form.append('image', imageFile.value)
    if (activeTab.value === 'url') form.append('url', urlInput.value)

    preview.value = await ingest(form)
    preview.value.source_type = activeTab.value
  } catch (e: any) {
    error.value = e.message
  } finally {
    loading.value = false
  }
}

async function handleSave(data: any) {
  try {
    const recipe = await createRecipe(data)
    router.push(`/recipes/${recipe.id}`)
  } catch (e: any) {
    error.value = e.message
  }
}
</script>
```

- [ ] **Step 3: Verify frontend builds**

Run: `cd frontend && npm run build`

- [ ] **Step 4: Commit**

```bash
git add frontend/src/
git commit -m "feat: recipe ingestion page — 3-tab input, AI parsing, editable preview"
```

---

### Task 27: Meal Plan + Log + Settings + Public Pages

**Files:**
- Create: `frontend/src/api/plan.ts`
- Create: `frontend/src/stores/plan.ts`
- Modify: `frontend/src/pages/PlanPage.vue`
- Modify: `frontend/src/pages/LogPage.vue`
- Modify: `frontend/src/pages/SettingsPage.vue`
- Modify: `frontend/src/pages/PublicRecipePage.vue`

- [ ] **Step 1: Write api/plan.ts**

```typescript
import { apiFetch } from './client'

export interface MealPlanEntry {
  id: string
  date: string
  meal_type: string
  recipe_id: string | null
  free_text: string | null
  servings: number | null
  status: string
  entry_type: string
  suggested_by_ai: boolean
  note: string | null
}

export function listPlan(from: string, to: string) {
  return apiFetch<MealPlanEntry[]>(`/plan?from=${from}&to=${to}`)
}

export function createPlanEntry(data: any) {
  return apiFetch<MealPlanEntry>('/plan', { method: 'POST', body: JSON.stringify(data) })
}

export function updatePlanEntry(id: string, data: any) {
  return apiFetch<MealPlanEntry>(`/plan/${id}`, { method: 'PUT', body: JSON.stringify(data) })
}

export function deletePlanEntry(id: string) {
  return apiFetch<void>(`/plan/${id}`, { method: 'DELETE' })
}

export function suggestPlan(prompt: string) {
  return apiFetch<any[]>('/plan/suggest', { method: 'POST', body: JSON.stringify({ prompt }) })
}
```

- [ ] **Step 2: Write PlanPage.vue**

```vue
<template>
  <div>
    <div class="flex items-center justify-between mb-6">
      <h1 class="text-2xl font-bold text-stone-800">Plan</h1>
      <div class="flex gap-2">
        <button @click="shiftWeek(-1)" class="px-3 py-1 border rounded-lg">←</button>
        <span class="px-3 py-1 text-stone-600">{{ weekLabel }}</span>
        <button @click="shiftWeek(1)" class="px-3 py-1 border rounded-lg">→</button>
      </div>
    </div>

    <!-- Suggest -->
    <div class="mb-6 flex gap-2">
      <input v-model="suggestPrompt" placeholder="Napr. Navrh jidla na tento tyden..."
        class="flex-1 px-4 py-2 border border-stone-300 rounded-lg" />
      <button @click="handleSuggest" :disabled="suggesting"
        class="px-4 py-2 bg-orange-600 text-white rounded-lg hover:bg-orange-700 disabled:opacity-50">
        Navrhnout
      </button>
    </div>

    <!-- Calendar grid -->
    <div class="space-y-4">
      <div v-for="day in days" :key="day.date" class="bg-white rounded-xl border border-stone-200 p-4">
        <h3 class="font-medium text-stone-700 mb-2">{{ formatDay(day.date) }}</h3>
        <div class="space-y-2">
          <div v-for="entry in day.entries" :key="entry.id"
            class="flex items-center justify-between px-3 py-2 rounded-lg"
            :class="entry.status === 'suggested' ? 'border-2 border-dashed border-orange-300 bg-orange-50' : 'bg-stone-50'">
            <span class="text-stone-800">
              <span class="text-stone-400 text-sm mr-2">{{ entry.meal_type }}</span>
              {{ entry.free_text || 'Recept' }}
            </span>
            <div class="flex gap-1">
              <button v-if="entry.status === 'suggested'" @click="confirmEntry(entry)"
                class="text-green-600 text-sm hover:underline">Potvrdit</button>
              <button @click="removeEntry(entry.id)" class="text-red-400 text-sm hover:underline">✕</button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import * as planApi from '../api/plan'

const weekOffset = ref(0)
const entries = ref<planApi.MealPlanEntry[]>([])
const suggestions = ref<any[]>([])
const suggestPrompt = ref('')
const suggesting = ref(false)

const startDate = computed(() => {
  const d = new Date()
  d.setDate(d.getDate() - d.getDay() + 1 + weekOffset.value * 7) // Monday
  return d
})

const weekLabel = computed(() => {
  const s = startDate.value
  const e = new Date(s)
  e.setDate(e.getDate() + 6)
  return `${fmt(s)} – ${fmt(e)}`
})

function fmt(d: Date) {
  return d.toISOString().slice(0, 10)
}

const days = computed(() => {
  const result = []
  for (let i = 0; i < 7; i++) {
    const d = new Date(startDate.value)
    d.setDate(d.getDate() + i)
    const date = fmt(d)
    const dayEntries = [
      ...entries.value.filter(e => e.date === date),
      ...suggestions.value.filter(s => s.date === date).map(s => ({ ...s, id: `sug-${s.date}-${s.meal_type}`, status: 'suggested' })),
    ]
    result.push({ date, entries: dayEntries })
  }
  return result
})

function formatDay(date: string) {
  const d = new Date(date)
  return d.toLocaleDateString('cs-CZ', { weekday: 'long', day: 'numeric', month: 'numeric' })
}

function shiftWeek(dir: number) {
  weekOffset.value += dir
  loadEntries()
}

async function loadEntries() {
  const from = fmt(startDate.value)
  const to = fmt(new Date(startDate.value.getTime() + 6 * 86400000))
  entries.value = await planApi.listPlan(from, to)
}

async function handleSuggest() {
  suggesting.value = true
  try {
    suggestions.value = await planApi.suggestPlan(suggestPrompt.value)
  } finally {
    suggesting.value = false
  }
}

async function confirmEntry(entry: any) {
  await planApi.createPlanEntry({
    date: entry.date,
    meal_type: entry.meal_type,
    recipe_id: entry.recipe_id,
    free_text: entry.free_text,
    note: entry.note,
    status: 'confirmed',
  })
  suggestions.value = suggestions.value.filter(s => !(s.date === entry.date && s.meal_type === entry.meal_type))
  await loadEntries()
}

async function removeEntry(id: string) {
  if (id.startsWith('sug-')) {
    suggestions.value = suggestions.value.filter(s => `sug-${s.date}-${s.meal_type}` !== id)
  } else {
    await planApi.deletePlanEntry(id)
    await loadEntries()
  }
}

onMounted(loadEntries)
</script>
```

- [ ] **Step 3: Write LogPage.vue**

```vue
<template>
  <div>
    <h1 class="text-2xl font-bold text-stone-800 mb-6">Co jsme dnes jedli?</h1>
    <div class="space-y-6">
      <div v-for="meal in ['lunch', 'dinner']" :key="meal" class="bg-white rounded-xl border border-stone-200 p-4">
        <h3 class="font-medium text-stone-700 mb-3">{{ meal === 'lunch' ? 'Obed' : 'Vecere' }}</h3>
        <input v-model="logs[meal]" :placeholder="`Co jste meli k ${meal === 'lunch' ? 'obedu' : 'veceri'}?`"
          class="w-full px-4 py-3 border border-stone-300 rounded-lg text-lg" />
      </div>
      <button @click="saveLog" :disabled="saving"
        class="w-full py-3 bg-orange-600 text-white rounded-lg hover:bg-orange-700 font-medium text-lg disabled:opacity-50">
        {{ saving ? 'Ukladam...' : 'Ulozit' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { reactive, ref } from 'vue'
import { createPlanEntry } from '../api/plan'

const logs = reactive<Record<string, string>>({ lunch: '', dinner: '' })
const saving = ref(false)

async function saveLog() {
  saving.value = true
  const today = new Date().toISOString().slice(0, 10)
  try {
    for (const [meal, text] of Object.entries(logs)) {
      if (text.trim()) {
        await createPlanEntry({
          date: today,
          meal_type: meal,
          free_text: text,
          entry_type: 'logged',
        })
      }
    }
    logs.lunch = ''
    logs.dinner = ''
    alert('Ulozeno!')
  } finally {
    saving.value = false
  }
}
</script>
```

- [ ] **Step 4: Write SettingsPage.vue**

```vue
<template>
  <div>
    <h1 class="text-2xl font-bold text-stone-800 mb-6">Nastaveni</h1>

    <section class="bg-white rounded-xl border border-stone-200 p-4 mb-6">
      <h2 class="font-semibold text-stone-700 mb-3">Dietni omezeni</h2>
      <div class="flex flex-wrap gap-2 mb-3">
        <span v-for="r in restrictions" :key="r"
          class="px-3 py-1 bg-orange-100 text-orange-700 rounded-full text-sm flex items-center gap-1">
          {{ r }}
          <button @click="removeRestriction(r)" class="hover:text-red-600">✕</button>
        </span>
      </div>
      <div class="flex gap-2">
        <input v-model="newRestriction" placeholder="Napr. vegetarian, bezlepkove..."
          class="flex-1 px-3 py-2 border border-stone-300 rounded-lg" @keyup.enter="addRestriction" />
        <button @click="addRestriction" class="px-4 py-2 bg-orange-600 text-white rounded-lg">Pridat</button>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useAuthStore } from '../stores/auth'
import { apiFetch } from '../api/client'

const authStore = useAuthStore()
const restrictions = ref<string[]>([])
const newRestriction = ref('')

async function loadRestrictions() {
  await authStore.fetchMe()
  restrictions.value = authStore.user?.dietary_restrictions || []
}

async function addRestriction() {
  if (!newRestriction.value.trim()) return
  await apiFetch('/settings/restrictions', {
    method: 'POST',
    body: JSON.stringify({ restriction: newRestriction.value.trim() }),
  })
  newRestriction.value = ''
  await loadRestrictions()
}

async function removeRestriction(r: string) {
  await apiFetch('/settings/restrictions', {
    method: 'DELETE',
    body: JSON.stringify({ restriction: r }),
  })
  await loadRestrictions()
}

onMounted(loadRestrictions)
</script>
```

- [ ] **Step 5: Write PublicRecipePage.vue**

```vue
<template>
  <div v-if="recipe">
    <h1 class="text-2xl font-bold text-stone-800 mb-2">{{ recipe.title }}</h1>
    <p v-if="recipe.description" class="text-stone-600 mb-4">{{ recipe.description }}</p>
    <TagChips v-if="recipe.tags?.length" :tags="recipe.tags" class="mb-6" />

    <section class="mb-8">
      <h2 class="text-lg font-semibold text-stone-700 mb-3">Ingredience</h2>
      <IngredientList :ingredients="recipe.ingredients || []" />
    </section>

    <section>
      <h2 class="text-lg font-semibold text-stone-700 mb-3">Postup</h2>
      <ol class="space-y-4">
        <li v-for="step in recipe.steps" :key="step.step_order" class="flex gap-3">
          <span class="flex-shrink-0 w-7 h-7 rounded-full bg-orange-100 text-orange-700 flex items-center justify-center text-sm font-medium">
            {{ step.step_order }}
          </span>
          <p class="text-stone-700 pt-0.5">{{ step.instruction }}</p>
        </li>
      </ol>
    </section>
  </div>
  <div v-else class="text-center text-stone-400 py-8">Nacitam...</div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRoute } from 'vue-router'
import { getPublicRecipe } from '../api/recipes'
import type { Recipe } from '../api/recipes'
import TagChips from '../components/TagChips.vue'
import IngredientList from '../components/IngredientList.vue'

const route = useRoute()
const recipe = ref<Recipe | null>(null)

onMounted(async () => {
  recipe.value = await getPublicRecipe(route.params.slug as string)
})
</script>
```

- [ ] **Step 6: Verify frontend builds**

Run: `cd frontend && npm run build`

- [ ] **Step 7: Commit**

```bash
git add frontend/src/
git commit -m "feat: plan, log, settings, public recipe pages"
```

---

## Phase 7: Static Serving + Dockerfile

### Task 28: Backend Static File Serving + SPA Fallback

**Files:**
- Modify: `backend/src/main.rs`

- [ ] **Step 1: Add static file serving and SPA fallback to create_router**

```rust
use tower_http::services::ServeDir;
use axum::response::Html;

pub fn create_router(state: AppState) -> Router {
    let api = Router::new()
        // Auth
        .route("/auth/login", post(routes::auth::login))
        .route("/auth/me", get(routes::auth::me))
        // Recipes
        .route("/recipes", get(routes::recipes::list).post(routes::recipes::create))
        .route("/recipes/{id}", get(routes::recipes::get).put(routes::recipes::update).delete(routes::recipes::delete))
        .route("/recipes/{id}/share", post(routes::recipes::share).delete(routes::recipes::unshare))
        // Ingestion
        .route("/ingest", post(routes::ingest::ingest))
        // Chat
        .route("/chat/{recipe_id}", post(routes::chat::chat))
        // Meal plan (suggest BEFORE {id} to avoid conflict)
        .route("/plan/suggest", post(routes::plan::suggest))
        .route("/plan/history", get(routes::plan::history))
        .route("/plan", get(routes::plan::list).post(routes::plan::create))
        .route("/plan/{id}", put(routes::plan::update).delete(routes::plan::delete))
        // Push
        .route("/push/subscribe", post(routes::push::subscribe))
        .route("/push/unsubscribe", post(routes::push::unsubscribe))
        // Settings
        .route("/settings/restrictions", post(routes::settings::add_restriction).delete(routes::settings::remove_restriction))
        // Public
        .route("/public/recipes/{slug}", get(routes::public::get_recipe_by_slug));

    let static_dir = state.config.static_dir.clone();
    let upload_dir = state.config.upload_dir.clone();

    Router::new()
        .nest("/api", api)
        .nest_service("/uploads", ServeDir::new(&upload_dir))
        .fallback_service(ServeDir::new(&static_dir).fallback(tower_http::services::ServeFile::new(format!("{static_dir}/index.html"))))
        .with_state(state)
}
```

The `fallback_service` with `ServeDir` + `ServeFile` fallback means:
- `/api/*` → API routes
- `/uploads/*` → uploaded images
- Any existing file in `static/` (JS, CSS, assets) → served directly
- Everything else → `static/index.html` (Vue Router handles it)

- [ ] **Step 2: Verify compilation**

Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add backend/src/main.rs
git commit -m "feat: static file serving + SPA fallback for Vue Router"
```

---

### Task 29: Dockerfile

**Files:**
- Create: `Dockerfile`

- [ ] **Step 1: Write the Dockerfile**

```dockerfile
# ── Stage 1: Build frontend ──────────────────────────────────────
FROM node:25-alpine3.22 AS frontend-build
WORKDIR /app/frontend
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ .
RUN npm run build
# Output: /app/frontend/dist/

# ── Stage 2: Build backend ───────────────────────────────────────
FROM rust:1.90-bookworm AS backend-build
WORKDIR /app/backend
COPY backend/Cargo.toml backend/Cargo.lock ./
# Cache dependencies layer
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release && rm -rf src
COPY backend/ .
ENV SQLX_OFFLINE=true
RUN cargo build --release

# ── Stage 3: Runtime ─────────────────────────────────────────────
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend-build /app/backend/target/release/cooking-app ./
COPY --from=frontend-build /app/frontend/dist/ ./static/
COPY backend/migrations/ ./migrations/

EXPOSE 8080
VOLUME ["/app/uploads"]
CMD ["./cooking-app"]
```

- [ ] **Step 2: Generate sqlx offline data**

Run: `cd backend && cargo sqlx prepare`

This generates `.sqlx/` directory with query metadata. Add it to git:

```bash
git add backend/.sqlx/
```

- [ ] **Step 3: Commit**

```bash
git add Dockerfile backend/.sqlx/
git commit -m "feat: multi-stage Dockerfile + sqlx offline data"
```

---

## Phase 8: Final Verification

### Task 30: Run All Tests + End-to-End Smoke Test

- [ ] **Step 1: Run all backend integration tests**

Run: `cd backend && cargo test -- --nocapture`
Expected: all tests pass (auth, recipes, public, meal_plan, push, settings)

- [ ] **Step 2: Build frontend**

Run: `cd frontend && npm run build`
Expected: builds successfully

- [ ] **Step 3: Start the backend with frontend served**

Run:
```bash
cd backend
cp ../frontend/dist/ ./static/ -r
export DATABASE_URL=postgresql://cooking:cooking@localhost:5432/cookingapp
export JWT_SECRET=test-secret
export ANTHROPIC_API_KEY=dummy
RUST_LOG=info cargo run
```

- [ ] **Step 4: Smoke test in browser**

Open `http://localhost:8080`:
- Login page renders
- After login, recipe list page loads
- Navigate to `/recipes/new` — 3-tab ingestion UI renders
- Navigate to `/plan` — calendar renders
- Navigate to `/settings` — dietary restrictions UI works

- [ ] **Step 5: Seed two users for production**

Create a seed script or run manually:
```sql
INSERT INTO users (name, email, password_hash) VALUES
  ('Jenda', 'jenda@example.com', '$2b$12$...'),
  ('Partner', 'partner@example.com', '$2b$12$...');
```

Generate password hashes with: `htpasswd -bnBC 12 "" 'your-password' | tr -d ':\n' | sed 's/$2y/$2b/'`

- [ ] **Step 6: Commit any fixes**

```bash
git add -A
git commit -m "chore: final fixes from end-to-end verification"
```
