# Cooking App – Project Specification

## Overview

A self-hosted, two-user cooking assistant app for managing recipes, planning meals, and logging what we ate. Uses Claude API as the AI backbone. Runs on a home server accessible from the internet via a single Docker container (nginx handled externally on the host).

---

## Users

Two fixed users (Jenda + přítelkyně). No public registration. Auth via simple login (email + password, bcrypt, JWT). Both users share the same recipe database.

---

## Tech Stack

| Layer | Technology |
|---|---|
| Backend | Rust / Axum |
| DB access | sqlx (async, compile-time checked queries) |
| Database | PostgreSQL 18 + pgvector |
| Frontend | Vue 3 + Vite + Tailwind CSS 4 (static export) |
| AI | Anthropic Claude API via direct reqwest HTTP calls |
| Container | Single Docker container, multi-stage build |

---

## Project Structure

```
/
├── backend/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs
│   │   ├── routes/
│   │   │   ├── auth.rs
│   │   │   ├── recipes.rs
│   │   │   ├── ingest.rs
│   │   │   ├── plan.rs
│   │   │   ├── chat.rs
│   │   │   ├── push.rs
│   │   │   └── public.rs
│   │   ├── models/
│   │   ├── ai/
│   │   │   ├── client.rs      # reqwest wrapper, auth headers
│   │   │   ├── ingest.rs      # recipe parsing prompts
│   │   │   ├── plan.rs        # meal planning prompts
│   │   │   └── chat.rs        # streaming chat + tool use
│   │   ├── push/              # Web Push / VAPID
│   │   └── db/                # sqlx queries
│   ├── migrations/
│   └── .sqlx/                 # offline query metadata for Docker builds
├── frontend/
│   ├── package.json
│   ├── vite.config.ts
│   └── src/
│       ├── pages/
│       ├── components/
│       └── stores/            # Pinia
├── docker-compose.yml       # local dev: Postgres
└── Dockerfile
```

---

## Dockerfile (multi-stage)

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
# /app/uploads must be a mounted volume for persistent image storage
VOLUME ["/app/uploads"]
CMD ["./cooking-app"]
```

The Axum server:
- serves the Vue SPA from `./static/` via `tower_http::services::ServeDir`
- serves `/api/*` routes
- serves `/r/:slug` → returns `index.html`, Vue Router handles it client-side

**sqlx offline mode:** During development, run `cargo sqlx prepare` against a live DB to generate `.sqlx/` query metadata. This is checked into git so Docker builds work without a live database.

---

## Local Development (docker-compose.yml)

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

Run `docker compose up -d`, then:
```bash
export DATABASE_URL=postgresql://cooking:cooking@localhost:5432/cookingapp
cd backend && cargo sqlx migrate run
```

---

## Rust Dependencies (Cargo.toml)

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
time = { version = "0.3", features = ["serde"] }
bcrypt = "0.17"
jsonwebtoken = "9"
tower-http = { version = "0.6", features = ["fs", "cors"] }
tower = "0.5"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
tokio-stream = "0.1"
futures = "0.3"
base64 = "0.22"
scraper = "0.22"
web-push = "0.10"

[dev-dependencies]
testcontainers = "0.24"
testcontainers-modules = { version = "0.12", features = ["postgres"] }
# Tests use pgvector/pgvector:pg18 image override — see tests/common/mod.rs
```

---

## Database Schema

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

Run migrations: `sqlx migrate run`

---

## Anthropic API Client (Rust)

All Claude calls go through a shared `AnthropicClient` struct:

```rust
// ai/client.rs
pub struct AnthropicClient {
    client: reqwest::Client,
    api_key: String,
}

impl AnthropicClient {
    // Single-shot completion
    pub async fn complete(
        &self, model: &str, system: &str,
        messages: Vec<Message>, max_tokens: u32
    ) -> anyhow::Result<String>

    // SSE stream for chat
    pub async fn stream(
        &self, model: &str, system: &str,
        messages: Vec<Message>, tools: Option<Vec<Tool>>
    ) -> anyhow::Result<impl Stream<Item = anyhow::Result<StreamEvent>>>
}
```

**Models:**
- `claude-haiku-4-5-20251001` – ingestion, meal plan suggestions
- `claude-sonnet-4-6` – recipe chat/editing

**Required headers:**
```
x-api-key: <ANTHROPIC_API_KEY>
anthropic-version: 2023-06-01
content-type: application/json
```

### Ingestion system prompt

```
You are a recipe parser. Extract the recipe from the user's input and return ONLY valid JSON.
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
Assign 1-5 tags.
```

- For `photo`: send image as base64 content block (`"type": "image"`)
- For `url`: fetch HTML server-side, strip to readable text with `scraper` crate, send as text

### Chat: update_recipe tool definition

```json
{
  "name": "update_recipe",
  "description": "Update fields of the current recipe based on the conversation",
  "input_schema": {
    "type": "object",
    "properties": {
      "title":         { "type": "string" },
      "description":   { "type": "string" },
      "servings":      { "type": "number" },
      "prep_time_min": { "type": "number" },
      "cook_time_min": { "type": "number" },
      "tags": {
        "type": "array",
        "items": { "type": "string" }
      },
      "ingredients": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "name":   { "type": "string" },
            "amount": { "type": "number" },
            "unit":   { "type": "string" },
            "note":   { "type": "string" }
          }
        }
      },
      "steps": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "step_order":  { "type": "number" },
            "instruction": { "type": "string" }
          }
        }
      }
    }
  }
}
```

Chat system prompt includes current recipe as JSON:
```
You are a cooking assistant helping edit a recipe. The current recipe is:
<recipe>{recipe_json}</recipe>
When the user asks to change something, respond conversationally AND call the
update_recipe tool with only the fields that changed.
```

### Chat SSE stream

`POST /api/chat/:recipe_id` returns `text/event-stream`.

Axum handler pipes Claude's SSE stream directly to the HTTP response.

Frontend handles two event types:
- `content_block_delta` + `type: text_delta` → append to chat bubble
- `input_json_delta` accumulated until `content_block_stop` → parse as `update_recipe` input → update recipe display live

User clicks "Uložit změny" to persist to DB.

### Meal plan suggestion prompt

```
You are a meal planning assistant. Suggest meals for the upcoming days.
Avoid repeating meals from recent history. Respect all dietary restrictions.
Prefer variety in tags — don't suggest three soups in a row.

Recent history (last 90 days):
<history>{history_json}</history>

Dietary restrictions:
<restrictions>{restrictions_json}</restrictions>

Available recipes (with tags):
<recipes>{recipe_list_json}</recipes>

User request: {prompt}

Return ONLY a valid JSON array:
[{
  "date": "YYYY-MM-DD",
  "meal_type": "lunch|dinner",
  "recipe_id": "uuid or null",
  "free_text": "string or null",
  "note": "string or null"
}]
```

---

## API Endpoints

### Auth
```
POST /api/auth/login     { email, password } → { token, user }
POST /api/auth/logout
GET  /api/auth/me        → user + dietary restrictions
```

### Recipes
```
GET    /api/recipes              → list (pagination + ?q= search + ?tag= filter)
POST   /api/recipes              → create
GET    /api/recipes/:id          → detail with ingredients + steps + tags
PUT    /api/recipes/:id          → full update
DELETE /api/recipes/:id
POST   /api/recipes/:id/share    → generate public_slug → { share_url }
DELETE /api/recipes/:id/share    → revoke public share
```

### Ingestion
```
POST /api/ingest    (multipart/form-data)
  source_type: "manual" | "photo" | "url"
  text?:       string
  image?:      file
  url?:        string
→ parsed recipe preview (not saved to DB)
```

### Recipe Chat
```
POST /api/chat/:recipe_id
  Body:    { message: string, session_id?: string }
  Returns: text/event-stream
```

### Meal Plan
```
GET    /api/plan?from=YYYY-MM-DD&to=YYYY-MM-DD
POST   /api/plan
PUT    /api/plan/:id
DELETE /api/plan/:id
POST   /api/plan/suggest    { prompt: string } → suggested entries (not saved)
GET    /api/plan/history?days=90
```

### Push Notifications
```
POST /api/push/subscribe      { subscription: object }
POST /api/push/unsubscribe    { subscription: object }
```

Backend runs a tokio background task (not an external cron) that wakes daily at `PUSH_NOTIFY_HOUR`. For each user with a push subscription, if no `dinner` entry exists for today → send push notification: "Co jste dnes měli k večeři?". Notification click opens `/log`.

### Public
```
GET /api/public/recipes/:slug   → recipe detail, no auth required
GET /r/:slug                    → returns index.html (Vue Router handles rendering)
```

---

## Frontend Pages (Vue 3 + Vue Router)

```
/               → redirect to /recipes
/login
/recipes        → list + search + tag filter chips
/recipes/new    → ingestion (3 tabs: Napsat / Fotka / Web)
                  → after parsing: editable preview with tag chips → confirm → save
/recipes/:id    → recipe detail
                  - ingredients + steps + tag chips
                  - cooking mode (steps one at a time, large font)
                  - floating chat button (bottom right)
                  - chat overlay: right panel (desktop) / bottom sheet (mobile)
                  - "Sdílet" button → copies share URL
/plan           → 7-day calendar grid
                  - confirmed entries: solid style
                  - suggested entries: dashed border, muted color
                  - per-slot: quick-add buttons + editable prompt field
                  - "Navrhnout" button → /api/plan/suggest → show suggestions
/r/:slug        → public read-only recipe view (no auth)
/log            → quick meal log (opened from push notification)
                  - pre-fills if today has confirmed entries
                  - two slots: oběd + večeře
                  - each: recipe autocomplete OR free-text toggle
/settings       → dietary restrictions per user, push toggle, notify hour
```

**State management:** Pinia
**HTTP:** native `fetch` API
**CSS:** Tailwind CSS 4 — utility-first, mobile-first breakpoints (`sm:`, `md:`, `lg:`)

---

## Key UI/UX Requirements

- **Mobile-first.** Recipe detail must be comfortable on a phone with greasy hands: large text, high contrast, big tap targets.
- **Cooking mode.** Steps displayed one at a time with large font, tap/swipe to advance.
- **Chat overlay.** Opens over recipe without navigation. Bottom sheet on mobile, right panel on desktop. Streaming text visible in real time. Recipe updates live on `update_recipe` tool call.
- **Ingestion preview.** Always show editable form pre-filled by Claude before saving. User confirms explicitly.
- **Plan suggestions.** Visually distinct from confirmed. Per-entry confirm/skip buttons. Never auto-save.

---

## Environment Variables

```env
DATABASE_URL=postgresql://user:pass@host:5432/cookingapp
ANTHROPIC_API_KEY=sk-ant-...
JWT_SECRET=...
JWT_EXPIRY_HOURS=720
VAPID_PUBLIC_KEY=...
VAPID_PRIVATE_KEY=...
VAPID_CONTACT=mailto:you@example.com
BASE_URL=https://your-domain.com
PUSH_NOTIFY_HOUR=20
STATIC_DIR=./static
UPLOAD_DIR=./uploads
SQLX_OFFLINE=true
```

**Image storage:** `UPLOAD_DIR` must point to a persistent location. In Docker, mount a host volume to `/app/uploads` (declared as `VOLUME` in the Dockerfile). Images are served by Axum at `/uploads/*` via `ServeDir`.

---

## Build Order

Build in this sequence – each phase produces working, testable software:

1. **DB + migrations** – sqlx migrate setup, schema, seed two users
2. **Auth** – login endpoint, JWT middleware, `/api/auth/me`
3. **Recipe CRUD** – manual create/read/update/delete + Vue list + detail pages
4. **Ingestion pipeline** – `/api/ingest` for manual, photo, URL + Vue preview UI
5. **Recipe detail UI** – mobile-optimized display, cooking mode
6. **Chat overlay** – SSE streaming, `update_recipe` tool, Vue overlay component
7. **Meal plan** – calendar view, manual entries, history log
8. **AI planning** – `/api/plan/suggest`, suggest → confirm flow in Vue
9. **Push notifications** – service worker, VAPID, tokio background task, `/log` page
10. **Public sharing** – slug generation, public route + API endpoint
11. **Settings page** – dietary restrictions, notification preferences

---

## Integration Tests (testcontainers)

All backend endpoints are covered by integration tests that spin up a real PostgreSQL container via `testcontainers`. No mocks for the database layer.

### Test infrastructure

```rust
// tests/common/mod.rs

/// Starts a Postgres testcontainer, runs migrations, seeds two test users,
/// and returns an Axum router + the test users' JWT tokens.
pub async fn setup() -> TestContext {
    let container = PostgresImage::default()
        .with_tag("pg18")
        .with_name("pgvector/pgvector")
        .start().await;
    let pool = PgPool::connect(&container.connection_string()).await;
    sqlx::migrate!().run(&pool).await;
    // seed two users, return their JWTs
    TestContext { pool, router, container, user1_token, user2_token }
}
```

Tests use `axum::test::TestClient` (or direct `router.oneshot()` calls) — no HTTP server needed.

### Required test coverage

**Auth flow:**
- Login with valid credentials → 200 + JWT
- Login with wrong password → 401
- Access protected endpoint without token → 401
- Access protected endpoint with expired/invalid token → 401
- `/api/auth/me` returns correct user + dietary restrictions

**Recipe CRUD:**
- Create recipe with ingredients, steps, tags → 201
- List recipes → paginated results
- List recipes with `?q=` search → filtered results
- List recipes with `?tag=` filter → filtered results
- Get recipe detail → includes ingredients, steps, tags
- Update recipe (title, ingredients, steps, tags) → 200
- Delete recipe → 204, confirm gone
- Both users see all recipes (shared DB)

**Recipe ingredients edge case:**
- Same ingredient appears twice in one recipe (e.g., flour for dough + flour for dusting) → works

**Ingestion (manual only — no AI in tests):**
- POST `/api/ingest` with `source_type: manual` + text → parsed preview returned
- POST `/api/ingest` with missing fields → 400

**Meal plan:**
- Create meal plan entry (with recipe_id) → 201
- Create meal plan entry (with free_text) → 201
- Create entry with neither recipe_id nor free_text → 400 (CHECK constraint)
- List entries by date range → filtered correctly
- Update entry status (confirmed → cooked) → 200
- Delete entry → 204

**Public sharing:**
- Share recipe → generates slug, returns share URL
- GET `/api/public/recipes/:slug` without auth → 200 + recipe detail
- GET `/api/public/recipes/:slug` for non-existent slug → 404
- Revoke share → slug no longer works

**Push subscriptions:**
- Subscribe → 201
- Unsubscribe → 200
- Duplicate subscribe → idempotent

**Settings / dietary restrictions:**
- Add restriction → persists
- Remove restriction → gone
- `/api/auth/me` reflects updated restrictions

### Running tests

```bash
# Requires Docker daemon running (for testcontainers)
cd backend && cargo test
```

Tests run in parallel — each test gets its own Postgres container instance (or uses transaction rollback for isolation within a shared container, depending on performance needs).

---

## Out of Scope (post-MVP)

- Rohlik.cz MCP integration – DB already structured for shopping list generation from `meal_plan_entries`
- Recipe ratings / comments
- Nutritional values
- Multi-tenancy / public registration
- Instagram ingestion – Instagram blocks server-side scraping; users can screenshot and use the photo ingestion path instead
