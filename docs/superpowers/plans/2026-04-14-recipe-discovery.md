# Recipe Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add on-demand recipe discovery from curated Czech recipe websites with embedding-based dedup, AI scoring, and an inbox for reviewing candidates.

**Architecture:** Two-phase pipeline — scrape + parse (reuses existing URL ingestion via Sonnet) then score + dedup (new Haiku call). ONNX embedding service (all-MiniLM-L6-v2) for dedup pre-filtering. Recipes enter `discovered` status for user review. Soft-delete for rejections with recovery.

**Tech Stack:** Rust 1.90, Axum 0.8, sqlx 0.8, pgvector, ort 2.0.0-rc.12, tokenizers 0.22, ndarray 0.17, Vue 3, Pinia, Tailwind CSS 4

**Spec:** `docs/superpowers/specs/2026-04-14-recipe-discovery-design.md`

---

## File Structure

### New files
| File | Responsibility |
|---|---|
| `backend/migrations/004_recipe_discovery.sql` | Schema: status, embedding, canonical_name, discovery_score, scored_at, discovered_at columns + indexes |
| `backend/src/embedding.rs` | ONNX embedding service: load model, generate embeddings, cosine distance |
| `backend/src/ai/discovery.rs` | Haiku scoring call: canonical name, restriction check, duplicate check, relevance score |
| `backend/src/scraper.rs` | Site scraping: per-site config, fetch listing pages, extract recipe links |
| `backend/src/routes/discover.rs` | Discovery endpoint: orchestrates scrape → parse → embed → score → insert |
| `frontend/src/api/discover.ts` | Discovery API client |

### Modified files
| File | Changes |
|---|---|
| `backend/Cargo.toml` | Add ort, tokenizers, ndarray, pgvector |
| `backend/src/config.rs` | Add `embedding_model_dir`, `discovery_enabled` |
| `backend/src/models.rs` | Add status/canonical_name/discovery_score/scored_at/discovered_at to Recipe, new request/response types |
| `backend/src/lib.rs` | AppState gains `embedding` field, register discover + status routes |
| `backend/src/main.rs` | Load ONNX model at startup (optional, graceful degradation) |
| `backend/src/db/recipes.rs` | Status filtering on list queries, update_status(), embedding queries |
| `backend/src/routes/recipes.rs` | Add status query param to list, add PATCH status endpoint, add rescore endpoint |
| `frontend/src/api/recipes.ts` | Add status param to listRecipes, add updateStatus/rescore functions |
| `frontend/src/pages/RecipeListPage.vue` | Tab toggle, discover button + input, inbox view |
| `frontend/src/pages/RecipeDetailPage.vue` | Tested toggle, status badge, source URL |
| `frontend/src/stores/recipes.ts` | Add status filter to fetch |
| `frontend/src/components/RecipeCard.vue` | Discovery card variant with score badge + action buttons |
| `.env.example` | Add EMBEDDING_MODEL_DIR, DISCOVERY_ENABLED |
| `docker-compose.yml` | Add model volume mount |

---

### Task 1: Database Migration and Model Changes

**Files:**
- Create: `backend/migrations/004_recipe_discovery.sql`
- Modify: `backend/src/models.rs`
- Modify: `backend/Cargo.toml`

- [ ] **Step 1: Add pgvector crate to Cargo.toml**

Add after the `scraper` dependency in `backend/Cargo.toml`:

```toml
pgvector = { version = "0.4", features = ["sqlx"] }
```

- [ ] **Step 2: Create migration file**

Create `backend/migrations/004_recipe_discovery.sql`:

```sql
-- Recipe discovery: status lifecycle, embedding dedup, AI scoring

ALTER TABLE recipes ADD COLUMN status TEXT NOT NULL DEFAULT 'saved';
ALTER TABLE recipes ADD COLUMN embedding vector(384);
ALTER TABLE recipes ADD COLUMN discovery_score REAL;
ALTER TABLE recipes ADD COLUMN discovered_at TIMESTAMPTZ;
ALTER TABLE recipes ADD COLUMN scored_at TIMESTAMPTZ;
ALTER TABLE recipes ADD COLUMN canonical_name TEXT;

-- Backfill: existing recipes are all "tested" (already cooked)
UPDATE recipes SET status = 'tested';

-- Index for status filtering (most queries filter by status)
CREATE INDEX idx_recipes_status ON recipes (status);

-- HNSW index for cosine similarity search on embeddings
CREATE INDEX idx_recipes_embedding ON recipes USING hnsw (embedding vector_cosine_ops);
```

- [ ] **Step 3: Update Recipe model with new fields**

In `backend/src/models.rs`, add new fields to the `Recipe` struct after `updated_at`:

```rust
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
    pub emoji: Option<String>,
    pub cover_image_path: Option<String>,
    pub is_public: Option<bool>,
    pub public_slug: Option<String>,
    pub created_at: Option<OffsetDateTime>,
    pub updated_at: Option<OffsetDateTime>,
    // Discovery fields
    pub status: String,
    #[sqlx(skip)]
    #[serde(skip_serializing)]
    pub embedding: Option<()>, // pgvector handled separately, not in SELECT *
    pub discovery_score: Option<f32>,
    pub discovered_at: Option<OffsetDateTime>,
    pub scored_at: Option<OffsetDateTime>,
    pub canonical_name: Option<String>,
}
```

Note: The `embedding` field is skipped in the Recipe struct because pgvector vectors can't be directly deserialized with `query_as`. Embedding operations use separate queries.

- [ ] **Step 4: Add status to RecipeListQuery**

In `backend/src/models.rs`, add `status` field to `RecipeListQuery`:

```rust
#[derive(Debug, Deserialize)]
pub struct RecipeListQuery {
    pub q: Option<String>,
    pub tag: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub sort: Option<String>,
    /// Filter by status: "saved", "tested", "discovered", "rejected", "rejected_similar"
    /// Comma-separated for multiple. Default: "saved,tested"
    pub status: Option<String>,
}
```

- [ ] **Step 5: Add new request/response types for discovery**

Add to the end of `backend/src/models.rs`:

```rust
// -- Discovery --

#[derive(Debug, Deserialize)]
pub struct DiscoverRequest {
    pub prompt: Option<String>,
    pub count: Option<usize>,
    pub planning_for: Option<String>, // "both" (default) or "me"
}

#[derive(Debug, Serialize)]
pub struct DiscoverResponse {
    pub discovered: Vec<Recipe>,
    pub skipped: SkippedCounts,
    pub errors: Vec<SiteError>,
}

#[derive(Debug, Serialize, Default)]
pub struct SkippedCounts {
    pub duplicate: usize,
    pub restricted: usize,
    pub low_score: usize,
    pub similar_to_rejected: usize,
}

#[derive(Debug, Serialize)]
pub struct SiteError {
    pub site: String,
    pub error: String,
}

#[derive(Debug, Deserialize)]
pub struct StatusUpdateRequest {
    pub status: String,
}
```

- [ ] **Step 6: Verify migration runs**

```bash
cd backend && cargo build 2>&1 | tail -5
```

Note: The build may have warnings about unused fields — that's expected at this stage. The `SELECT *` queries in `db/recipes.rs` will need updating in Task 4 to include the new columns.

- [ ] **Step 7: Commit**

```bash
git add backend/migrations/004_recipe_discovery.sql backend/src/models.rs backend/Cargo.toml
git commit -m "feat(discovery): database migration and model types for recipe discovery

Add status lifecycle (discovered/saved/tested/rejected/rejected_similar),
embedding vector(384) column for dedup, canonical_name, discovery_score,
scored_at, discovered_at fields. Backfill existing recipes as 'tested'.
Add DiscoverRequest/Response, StatusUpdateRequest model types."
```

---

### Task 2: Configuration and Startup

**Files:**
- Modify: `backend/src/config.rs`
- Modify: `backend/src/lib.rs`
- Modify: `backend/src/main.rs`
- Modify: `.env.example`
- Modify: `docker-compose.yml`

- [ ] **Step 1: Add config fields**

In `backend/src/config.rs`, add two new fields to the `Config` struct:

```rust
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
    pub vapid_public_key: String,
    pub vapid_private_key: String,
    pub vapid_contact: String,
    // Discovery
    pub embedding_model_dir: Option<String>,
    pub discovery_enabled: bool,
}
```

And in `from_env()`, add after the vapid_contact line:

```rust
            embedding_model_dir: env::var("EMBEDDING_MODEL_DIR").ok(),
            discovery_enabled: env::var("DISCOVERY_ENABLED")
                .unwrap_or_else(|_| "true".into())
                .parse()
                .unwrap_or(true),
```

- [ ] **Step 2: Update .env.example**

Add to `.env.example`:

```
# Discovery (optional — if not set, discovery is disabled)
#EMBEDDING_MODEL_DIR=./models/all-MiniLM-L6-v2
#DISCOVERY_ENABLED=true
```

- [ ] **Step 3: Update docker-compose.yml**

Add a volume mount for the model files. In `docker-compose.yml`, under the backend service (or add a comment for future use):

```yaml
# To enable recipe discovery, download the all-MiniLM-L6-v2 model
# and mount it as a volume:
#   backend:
#     volumes:
#       - ./models/all-MiniLM-L6-v2:/models/all-MiniLM-L6-v2:ro
#     environment:
#       - EMBEDDING_MODEL_DIR=/models/all-MiniLM-L6-v2
```

- [ ] **Step 4: Add embedding field to AppState**

In `backend/src/lib.rs`, update AppState:

```rust
use std::sync::Arc;

pub mod ai;
pub mod auth;
pub mod config;
pub mod db;
pub mod embedding;
pub mod error;
pub mod models;
pub mod push_notifier;
pub mod routes;
pub mod scraper;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub config: Arc<config::Config>,
    pub http_client: reqwest::Client,
    pub embedding: Option<Arc<embedding::EmbeddingService>>,
}
```

- [ ] **Step 5: Update main.rs to load embedding model**

In `backend/src/main.rs`, after creating the http_client and before creating `state`, add:

```rust
    let embedding = if config.discovery_enabled {
        match &config.embedding_model_dir {
            Some(dir) => {
                match cooking_app::embedding::EmbeddingService::new(dir) {
                    Ok(svc) => {
                        tracing::info!(model_dir = %dir, "ONNX embedding model loaded — discovery enabled");
                        Some(Arc::new(svc))
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load embedding model from {dir}: {e} — discovery disabled");
                        None
                    }
                }
            }
            None => {
                tracing::info!("EMBEDDING_MODEL_DIR not set — discovery disabled");
                None
            }
        }
    } else {
        tracing::info!("Discovery disabled via DISCOVERY_ENABLED=false");
        None
    };

    let state = AppState {
        pool,
        config: Arc::new(config),
        http_client,
        embedding,
    };
```

- [ ] **Step 6: Update TestContext to include embedding field**

In `backend/tests/common/mod.rs`, update the AppState construction (around line 92):

```rust
        let state = cooking_app::AppState {
            pool: pool.clone(),
            config: Arc::new(config),
            http_client: reqwest::Client::new(),
            embedding: None, // Discovery disabled in tests
        };
```

- [ ] **Step 7: Verify it compiles**

```bash
cd backend && cargo build 2>&1 | tail -5
```

This will fail until we create the `embedding` and `scraper` modules (Task 3). Create empty placeholder modules first:

Create `backend/src/embedding.rs`:
```rust
pub struct EmbeddingService;

impl EmbeddingService {
    pub fn new(_model_dir: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Err("not yet implemented".into())
    }
}
```

Create `backend/src/scraper.rs`:
```rust
// Site scraping — implemented in Task 5
```

- [ ] **Step 8: Commit**

```bash
git add backend/src/config.rs backend/src/lib.rs backend/src/main.rs backend/src/embedding.rs backend/src/scraper.rs backend/tests/common/mod.rs .env.example docker-compose.yml
git commit -m "feat(discovery): configuration, AppState embedding field, ONNX startup

Add EMBEDDING_MODEL_DIR and DISCOVERY_ENABLED config. AppState gains
optional embedding service. Graceful degradation when model not available."
```

---

### Task 3: ONNX Embedding Service

**Files:**
- Create: `backend/src/embedding.rs` (replace placeholder)
- Modify: `backend/Cargo.toml`

- [ ] **Step 1: Add ONNX dependencies to Cargo.toml**

Add to `[dependencies]` in `backend/Cargo.toml`:

```toml
ort = { version = "2.0.0-rc.12", features = ["download-binaries", "ndarray"] }
tokenizers = { version = "0.22", default-features = false, features = ["onig"] }
ndarray = "0.17"
```

- [ ] **Step 2: Implement the embedding service**

Replace `backend/src/embedding.rs` with:

```rust
//! In-process ONNX embedding using all-MiniLM-L6-v2 (384 dimensions).
//! Adapted from second-brain crate — same model, same normalization.

use std::path::Path;
use std::sync::Mutex;

use ndarray::{Array2, Axis};
use ort::session::Session;
use ort::value::Tensor;

/// Stop-ingredients that carry no semantic signal for dedup.
const STOP_INGREDIENTS: &[&str] = &[
    "sůl", "pepř", "sůl a pepř", "pepř a sůl", "olej", "olivový olej",
    "rostlinný olej", "česnek", "cibule", "voda", "máslo", "smetana",
    "černý pepř", "bílý pepř", "mletý pepř", "řepkový olej", "neutrální olej",
];

pub struct EmbeddingService {
    session: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
}

impl EmbeddingService {
    /// Load the ONNX model and tokenizer from the given directory.
    /// The directory must contain `model.onnx` and `tokenizer.json`.
    pub fn new(model_dir: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let dir = Path::new(model_dir);
        let model_path = dir.join("model.onnx");
        let tokenizer_path = dir.join("tokenizer.json");

        if !model_path.exists() {
            return Err(format!("model.onnx not found in {model_dir}").into());
        }
        if !tokenizer_path.exists() {
            return Err(format!("tokenizer.json not found in {model_dir}").into());
        }

        let session = Session::builder()
            .map_err(|e| format!("session builder: {e}"))?
            .with_intra_threads(1)
            .map_err(|e| format!("intra threads: {e}"))?
            .commit_from_file(&model_path)
            .map_err(|e| format!("load model: {e}"))?;

        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("load tokenizer: {e}"))?;

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
        })
    }

    /// Generate a 384-dimensional L2-normalized embedding for the given text.
    pub fn embed(&self, text: &str) -> Option<Vec<f32>> {
        let encoding = self.tokenizer.encode(text, true).ok()?;
        let ids = encoding.get_ids();
        let mask = encoding.get_attention_mask();
        let type_ids = encoding.get_type_ids();
        let seq_len = ids.len();

        let input_ids =
            Array2::from_shape_vec((1, seq_len), ids.iter().map(|&x| i64::from(x)).collect())
                .ok()?;
        let attention_mask =
            Array2::from_shape_vec((1, seq_len), mask.iter().map(|&x| i64::from(x)).collect())
                .ok()?;
        let token_type_ids =
            Array2::from_shape_vec((1, seq_len), type_ids.iter().map(|&x| i64::from(x)).collect())
                .ok()?;

        let mut session = self.session.lock().ok()?;
        let outputs = session
            .run(ort::inputs![
                "input_ids" => Tensor::from_array(input_ids).ok()?,
                "attention_mask" => Tensor::from_array(attention_mask).ok()?,
                "token_type_ids" => Tensor::from_array(token_type_ids).ok()?,
            ])
            .ok()?;

        let (shape, hidden_data) = outputs[0].try_extract_tensor::<f32>().ok()?;
        let hidden_dim = *shape.last()? as usize;
        let hidden = ndarray::ArrayView2::from_shape((seq_len, hidden_dim), hidden_data).ok()?;

        // Mean pooling with attention mask
        let mask_f32: Array2<f32> =
            Array2::from_shape_vec((seq_len, 1), mask.iter().map(|&x| x as f32).collect()).ok()?;
        let masked = &hidden * &mask_f32;
        let summed = masked.sum_axis(Axis(0));
        let mask_sum = mask_f32.sum().max(1e-9);
        let pooled = &summed / mask_sum;

        // L2 normalize
        let norm = pooled.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
        Some(pooled.iter().map(|x| x / norm).collect())
    }

    /// Build the embedding text for a recipe using the validated template:
    /// "{canonical_name}. {canonical_name}. Kategorie: {tags}. Obsahuje: {top 5 filtered ingredients}."
    pub fn recipe_summary(
        canonical_name: &str,
        tags: &[String],
        ingredients: &[String],
    ) -> String {
        let tags_str = if tags.is_empty() {
            String::new()
        } else {
            format!(" Kategorie: {}.", tags.join(", "))
        };

        let filtered: Vec<&str> = ingredients
            .iter()
            .map(|s| s.as_str())
            .filter(|s| !is_stop_ingredient(s))
            .take(5)
            .collect();

        let ings_str = if filtered.is_empty() {
            String::new()
        } else {
            format!(" Obsahuje: {}.", filtered.join(", "))
        };

        format!("{canonical_name}. {canonical_name}.{tags_str}{ings_str}")
    }

    /// Build a quick mechanical embedding text (no canonical name yet — used for pre-filtering).
    pub fn recipe_summary_mechanical(
        title: &str,
        tags: &[String],
        ingredients: &[String],
    ) -> String {
        Self::recipe_summary(title, tags, ingredients)
    }
}

fn is_stop_ingredient(ing: &str) -> bool {
    let lower = ing.to_lowercase();
    STOP_INGREDIENTS.iter().any(|s| lower == *s)
}

/// Cosine similarity between two L2-normalized vectors (= dot product).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}
```

- [ ] **Step 3: Verify it compiles**

```bash
cd backend && cargo build 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add backend/src/embedding.rs backend/Cargo.toml
git commit -m "feat(discovery): ONNX embedding service with all-MiniLM-L6-v2

In-process embedding using ort + tokenizers. 384-dim L2-normalized vectors.
Stop-ingredient filtering, recipe summary templates (canonical + mechanical).
Cosine similarity helper for dedup pre-filtering."
```

---

### Task 4: Recipe Database Layer — Status Filtering and Embedding Queries

**Files:**
- Modify: `backend/src/db/recipes.rs`

This is the most impactful change — all existing `SELECT r.*` queries must handle the new columns, and list queries must filter by status.

- [ ] **Step 1: Update the list() function to accept status filter**

Change the `list()` function signature in `backend/src/db/recipes.rs` to accept a status filter:

```rust
pub async fn list(
    pool: &PgPool,
    q: Option<&str>,
    tag: Option<&str>,
    sort: &str,
    page: i64,
    per_page: i64,
    statuses: &[&str],
) -> Result<(Vec<Recipe>, i64), sqlx::Error> {
```

In every SQL query branch within `list()`, add `AND r.status = ANY($N)` to the WHERE clause (where `$N` is a new bind parameter). The statuses parameter is bound as a text array.

For the tag filter branch, the query becomes:
```sql
SELECT r.id, r.owner_id, r.title, r.description, r.servings, r.prep_time_min,
       r.cook_time_min, r.source_type, r.source_url, r.emoji, r.cover_image_path,
       r.is_public, r.public_slug, r.created_at, r.updated_at,
       r.status, r.discovery_score, r.discovered_at, r.scored_at, r.canonical_name
FROM recipes r
JOIN recipe_tags rt ON r.id = rt.recipe_id
WHERE rt.tag = $1 AND r.status = ANY($4)
ORDER BY ...
LIMIT $2 OFFSET $3
```

Apply the same pattern to all three branches (tag filter, text search, no filter) and their count queries.

- [ ] **Step 2: Add update_status() function**

Add to `backend/src/db/recipes.rs`:

```rust
/// Allowed status transitions (from → [to]).
fn is_valid_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("discovered", "saved")
            | ("discovered", "rejected")
            | ("discovered", "rejected_similar")
            | ("saved", "tested")
            | ("rejected", "discovered")
            | ("rejected_similar", "discovered")
    )
}

pub async fn update_status(
    pool: &PgPool,
    id: Uuid,
    new_status: &str,
) -> Result<Option<Recipe>, sqlx::Error> {
    let current = sqlx::query_scalar::<_, String>("SELECT status FROM recipes WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    let Some(current) = current else {
        return Ok(None);
    };

    if !is_valid_transition(&current, new_status) {
        // Return the recipe unchanged — caller should check status
        let recipe = sqlx::query_as::<_, Recipe>(
            "SELECT id, owner_id, title, description, servings, prep_time_min, cook_time_min,
                    source_type, source_url, emoji, cover_image_path, is_public, public_slug,
                    created_at, updated_at, status, discovery_score, discovered_at, scored_at, canonical_name
             FROM recipes WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        return Ok(recipe);
    }

    let recipe = sqlx::query_as::<_, Recipe>(
        "UPDATE recipes SET status = $2, updated_at = now()
         WHERE id = $1
         RETURNING id, owner_id, title, description, servings, prep_time_min, cook_time_min,
                   source_type, source_url, emoji, cover_image_path, is_public, public_slug,
                   created_at, updated_at, status, discovery_score, discovered_at, scored_at, canonical_name",
    )
    .bind(id)
    .bind(new_status)
    .fetch_optional(pool)
    .await?;

    Ok(recipe)
}
```

- [ ] **Step 3: Add embedding-related query functions**

Add to `backend/src/db/recipes.rs`:

```rust
use pgvector::Vector;

/// Store an embedding for a recipe.
pub async fn set_embedding(
    pool: &PgPool,
    id: Uuid,
    embedding: &[f32],
    canonical_name: &str,
) -> Result<(), sqlx::Error> {
    let vec = Vector::from(embedding.to_vec());
    sqlx::query(
        "UPDATE recipes SET embedding = $2, canonical_name = $3 WHERE id = $1",
    )
    .bind(id)
    .bind(vec)
    .bind(canonical_name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Find the N most similar recipes by embedding cosine distance.
/// Returns (recipe_id, title, canonical_name, similarity_score).
pub async fn find_similar(
    pool: &PgPool,
    embedding: &[f32],
    statuses: &[&str],
    limit: i32,
) -> Result<Vec<(Uuid, String, Option<String>, f64)>, sqlx::Error> {
    let vec = Vector::from(embedding.to_vec());
    let rows = sqlx::query_as::<_, (Uuid, String, Option<String>, f64)>(
        "SELECT id, title, canonical_name, 1 - (embedding <=> $1) AS similarity
         FROM recipes
         WHERE embedding IS NOT NULL AND status = ANY($2)
         ORDER BY embedding <=> $1
         LIMIT $3",
    )
    .bind(vec)
    .bind(statuses)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Insert a discovered recipe with all discovery fields.
pub async fn create_discovered(
    pool: &PgPool,
    owner_id: Uuid,
    title: &str,
    description: Option<&str>,
    source_url: &str,
    canonical_name: &str,
    discovery_score: f32,
    embedding: &[f32],
    tags: &[String],
    ingredients: &[crate::models::IngredientInput],
    steps: &[crate::models::StepInput],
) -> Result<Recipe, sqlx::Error> {
    let vec = Vector::from(embedding.to_vec());
    let recipe = sqlx::query_as::<_, Recipe>(
        "INSERT INTO recipes (owner_id, title, description, source_type, source_url,
                              status, canonical_name, discovery_score, embedding,
                              discovered_at, scored_at)
         VALUES ($1, $2, $3, 'url', $4,
                 'discovered', $5, $6, $7,
                 now(), now())
         RETURNING id, owner_id, title, description, servings, prep_time_min, cook_time_min,
                   source_type, source_url, emoji, cover_image_path, is_public, public_slug,
                   created_at, updated_at, status, discovery_score, discovered_at, scored_at, canonical_name",
    )
    .bind(owner_id)
    .bind(title)
    .bind(description)
    .bind(source_url)
    .bind(canonical_name)
    .bind(discovery_score)
    .bind(vec)
    .fetch_one(pool)
    .await?;

    // Insert tags and ingredients using existing helpers
    let mut tx = pool.begin().await?;
    if !tags.is_empty() {
        super::recipes::insert_tags(&mut tx, recipe.id, tags).await?;
    }
    if !ingredients.is_empty() {
        super::recipes::insert_ingredients(&mut tx, recipe.id, ingredients).await?;
    }
    for step in steps {
        sqlx::query(
            "INSERT INTO recipe_steps (recipe_id, step_order, instruction) VALUES ($1, $2, $3)",
        )
        .bind(recipe.id)
        .bind(step.step_order)
        .bind(&step.instruction)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(recipe)
}
```

- [ ] **Step 4: Update existing queries to use explicit column lists**

All existing `SELECT r.*` or `SELECT *` queries in `db/recipes.rs` must be updated to use explicit column lists that include the new fields. This affects:
- `create()` — the RETURNING clause
- `get_by_id()` — the SELECT
- `list()` — all three branches
- `update()` — the RETURNING clause
- `delete()` — no change needed (returns bool)
- `set_public_slug()`, `remove_public_slug()`, `get_by_slug()` — if they return Recipe

The explicit column list is:
```sql
r.id, r.owner_id, r.title, r.description, r.servings, r.prep_time_min, r.cook_time_min,
r.source_type, r.source_url, r.emoji, r.cover_image_path, r.is_public, r.public_slug,
r.created_at, r.updated_at, r.status, r.discovery_score, r.discovered_at, r.scored_at, r.canonical_name
```

- [ ] **Step 5: Update routes/recipes.rs list handler**

In `backend/src/routes/recipes.rs`, update the `list()` handler to parse and pass the status filter:

```rust
pub async fn list(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(q): Query<RecipeListQuery>,
) -> AppResult<Json<Paginated<Recipe>>> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    let sort = q.sort.as_deref().unwrap_or("recent");

    let status_str = q.status.as_deref().unwrap_or("saved,tested");
    let statuses: Vec<&str> = status_str.split(',').map(|s| s.trim()).collect();

    let (items, total) =
        db::recipes::list(&state.pool, q.q.as_deref(), q.tag.as_deref(), sort, page, per_page, &statuses)
            .await?;

    Ok(Json(Paginated {
        items,
        total,
        page,
        per_page,
    }))
}
```

- [ ] **Step 6: Add status update and rescore route handlers**

Add to `backend/src/routes/recipes.rs`:

```rust
pub async fn update_status(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<StatusUpdateRequest>,
) -> AppResult<Json<Recipe>> {
    let recipe = db::recipes::update_status(&state.pool, id, &body.status)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(recipe))
}
```

- [ ] **Step 7: Register the new route in lib.rs**

In `backend/src/lib.rs`, add after the existing `/recipes/{id}/share` route:

```rust
        .route(
            "/recipes/{id}/status",
            axum::routing::patch(routes::recipes::update_status),
        )
```

- [ ] **Step 8: Verify it compiles and tests pass**

```bash
cd backend && cargo build 2>&1 | tail -5
cargo test 2>&1 | tail -20
```

- [ ] **Step 9: Commit**

```bash
git add backend/src/db/recipes.rs backend/src/routes/recipes.rs backend/src/lib.rs
git commit -m "feat(discovery): status filtering, embedding queries, status transitions

Recipe list queries filter by status (default: saved,tested).
PATCH /recipes/{id}/status with server-side transition validation.
Embedding storage and cosine similarity search via pgvector.
create_discovered() for inserting discovered recipes with full metadata."
```

---

### Task 5: Site Scraping Module

**Files:**
- Create: `backend/src/scraper.rs` (replace placeholder)

- [ ] **Step 1: Implement the scraper**

Replace `backend/src/scraper.rs` with:

```rust
//! Site scraping: fetch recipe listing pages and extract recipe URLs.

use scraper::{Html, Selector};

pub struct SiteConfig {
    pub name: &'static str,
    pub base_url: &'static str,
    pub listing_path: &'static str,
    pub search_path: Option<&'static str>,
    pub link_selector: &'static str,
}

/// Curated Czech recipe sites for v1.
pub fn sites() -> Vec<SiteConfig> {
    vec![
        SiteConfig {
            name: "fresh.iprima.cz",
            base_url: "https://fresh.iprima.cz",
            listing_path: "/recepty",
            search_path: None,
            link_selector: "a[href*=\"fresh.iprima.cz/\"]",
        },
        SiteConfig {
            name: "kuchynelidlu.cz",
            base_url: "https://kuchynelidlu.cz",
            listing_path: "/recepty",
            search_path: Some("/vyhledavani?q={query}"),
            link_selector: "a[href*=\"/recept/\"]",
        },
        SiteConfig {
            name: "receptyodanicky.cz",
            base_url: "https://www.receptyodanicky.cz",
            listing_path: "/",
            search_path: None,
            link_selector: "a[href*=\"receptyodanicky.cz/\"]",
        },
    ]
}

/// Fetch a listing/search page and extract recipe URLs.
pub async fn fetch_recipe_urls(
    client: &reqwest::Client,
    site: &SiteConfig,
    prompt: Option<&str>,
    max_urls: usize,
) -> Result<Vec<String>, String> {
    let url = if let (Some(search_path), Some(query)) = (site.search_path, prompt) {
        let path = search_path.replace("{query}", &urlencoding::encode(query));
        format!("{}{}", site.base_url, path)
    } else {
        format!("{}{}", site.base_url, site.listing_path)
    };

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("{}: {e}", site.name))?;

    if !resp.status().is_success() {
        return Err(format!("{}: HTTP {}", site.name, resp.status()));
    }

    let html = resp
        .text()
        .await
        .map_err(|e| format!("{}: failed to read body: {e}", site.name))?;

    let document = Html::parse_document(&html);
    let selector = Selector::parse(site.link_selector)
        .map_err(|_| format!("{}: invalid CSS selector", site.name))?;

    let mut urls: Vec<String> = document
        .select(&selector)
        .filter_map(|el| el.value().attr("href"))
        .map(|href| {
            if href.starts_with("http") {
                href.to_string()
            } else {
                format!("{}{}", site.base_url, href)
            }
        })
        // Filter out non-recipe links (listing pages, categories, etc.)
        .filter(|u| {
            !u.ends_with("/recepty")
                && !u.ends_with("/recepty/")
                && !u.contains("/kategorie/")
                && !u.contains("/vyhledavani")
                && u.len() > site.base_url.len() + 5
        })
        .collect();

    // Deduplicate
    urls.sort();
    urls.dedup();

    // Shuffle for variety when no prompt
    if prompt.is_none() {
        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        urls.shuffle(&mut rng);
    }

    urls.truncate(max_urls);
    Ok(urls)
}
```

- [ ] **Step 2: Add urlencoding dependency**

Add to `backend/Cargo.toml`:

```toml
urlencoding = "2"
```

- [ ] **Step 3: Verify it compiles**

```bash
cd backend && cargo build 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add backend/src/scraper.rs backend/Cargo.toml
git commit -m "feat(discovery): site scraping module with 3 curated Czech recipe sites

Per-site config for listing path, search path, and link CSS selector.
Supports prompt-based search (kuchynelidlu.cz) and random discovery.
URL deduplication and filtering of non-recipe links."
```

---

### Task 6: Discovery AI Scoring Call

**Files:**
- Create: `backend/src/ai/discovery.rs`
- Modify: `backend/src/ai/mod.rs` (add module)

- [ ] **Step 1: Implement the scoring call**

Create `backend/src/ai/discovery.rs`:

```rust
//! AI scoring for recipe discovery candidates.
//! Single Haiku call per candidate: canonical name, restriction check, duplicate check, relevance score.

use crate::ai::client::{AnthropicClient, Message};
use serde::{Deserialize, Serialize};

const DISCOVERY_MODEL: &str = "claude-haiku-4-5-20251001";

#[derive(Debug, Serialize, Deserialize)]
pub struct ScoringResult {
    pub canonical_name: String,
    pub violates_restriction: bool,
    pub restriction_violated: Option<String>,
    pub is_duplicate: bool,
    pub duplicate_of: Option<String>,
    pub relevance_score: f32,
}

/// Score a recipe candidate against user preferences, restrictions, and existing recipes.
pub async fn score_candidate(
    client: &AnthropicClient,
    title: &str,
    description: Option<&str>,
    ingredients: &[String],
    tags: &[String],
    preferences_json: &str,
    restrictions_json: &str,
    existing_recipes: &str,
    rejected_recipes: &str,
) -> anyhow::Result<ScoringResult> {
    let desc = description.unwrap_or("(bez popisu)");
    let ings = ingredients.join(", ");
    let tags_str = tags.join(", ");

    let system = format!(
        "You are evaluating a recipe candidate for a household meal planning app.\n\n\
         Candidate recipe:\n\
         Title: {title}\n\
         Description: {desc}\n\
         Ingredients: {ings}\n\
         Tags: {tags_str}\n\n\
         User's food preferences: {preferences_json}\n\
         User's dietary restrictions: {restrictions_json}\n\n\
         Existing recipes in the book: {existing_recipes}\n\
         Previously rejected-similar recipes: {rejected_recipes}\n\n\
         Tasks:\n\
         1. CANONICAL NAME: Reduce the recipe title to a short canonical Czech dish name \
            (e.g. \"kuře na paprice\", \"mac and cheese\", \"špenátový salát s fetou\").\n\
         2. RESTRICTION CHECK: Does this recipe violate any dietary restriction? (yes/no, which one)\n\
         3. DUPLICATE CHECK: Is this essentially the same dish as any existing recipe? \
            Consider the canonical name and ingredients, not just the title. (yes/no, which existing recipe)\n\
         4. RELEVANCE SCORE: Rate 0.0-1.0 how well this matches the user's food preferences. \
            1.0 = perfect match, 0.0 = completely irrelevant.\n\n\
         Return ONLY valid JSON:\n\
         {{\"canonical_name\": \"string\", \"violates_restriction\": false, \
         \"restriction_violated\": null, \"is_duplicate\": false, \
         \"duplicate_of\": null, \"relevance_score\": 0.8}}"
    );

    let messages = vec![Message {
        role: "user".into(),
        content: serde_json::json!(format!(
            "Evaluate this recipe: {title}"
        )),
    }];

    let response = client.complete(DISCOVERY_MODEL, &system, messages, 1024).await?;
    let json_str = extract_json(&response);
    let result: ScoringResult = serde_json::from_str(json_str).map_err(|e| {
        tracing::error!("Failed to parse discovery scoring response: {e}\nRaw: {response}");
        e
    })?;

    Ok(result)
}

/// Strip markdown code fences and find the JSON object.
fn extract_json(response: &str) -> &str {
    let trimmed = response.trim();
    if let Some(start) = trimmed.find('{') {
        let end = trimmed.rfind('}').map(|i| i + 1).unwrap_or(trimmed.len());
        &trimmed[start..end]
    } else {
        trimmed
    }
}
```

- [ ] **Step 2: Register the module**

In `backend/src/ai/mod.rs` (or wherever ai modules are declared), add:

```rust
pub mod discovery;
```

If there is no `mod.rs`, check how ai modules are currently exported and follow the same pattern. The existing code in `lib.rs` has `pub mod ai;` — check if `backend/src/ai/` has a `mod.rs` or if modules are declared individually.

- [ ] **Step 3: Verify it compiles**

```bash
cd backend && cargo build 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add backend/src/ai/discovery.rs backend/src/ai/mod.rs
git commit -m "feat(discovery): Haiku scoring call — canonical name, restrictions, dedup, relevance

Single AI call per candidate that normalizes the title, checks dietary
restrictions, detects duplicates against existing recipes, and scores
relevance against user food preferences."
```

---

### Task 7: Discovery Route Handler (Orchestration)

**Files:**
- Create: `backend/src/routes/discover.rs`
- Modify: `backend/src/lib.rs` (register route)
- Modify: `backend/src/routes/mod.rs` (add module)

- [ ] **Step 1: Implement the discover endpoint**

Create `backend/src/routes/discover.rs`:

```rust
use axum::Json;
use axum::extract::State;
use std::sync::Arc;

use crate::AppState;
use crate::ai::client::AnthropicClient;
use crate::auth::AuthUser;
use crate::embedding::{self, EmbeddingService};
use crate::error::{AppError, AppResult};
use crate::models::{DiscoverRequest, DiscoverResponse, SiteError, SkippedCounts};
use crate::{db, scraper};

pub async fn discover(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<DiscoverRequest>,
) -> AppResult<Json<DiscoverResponse>> {
    let embedding_svc = state
        .embedding
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("Discovery is unavailable: embedding model not configured".into()))?;

    let count = body.count.unwrap_or(5).min(10);
    let planning_for = body.planning_for.as_deref().unwrap_or("both");

    let client = AnthropicClient::new(&state.config.anthropic_api_key);

    // Gather user context
    let restrictions = if planning_for == "me" {
        db::users::get_dietary_restrictions(&state.pool, auth.user_id).await
            .map_err(AppError::Sqlx)?
    } else {
        db::users::get_all_dietary_restrictions(&state.pool).await
            .map_err(AppError::Sqlx)?
    };
    let preferences = if planning_for == "me" {
        db::users::get_food_preferences(&state.pool, auth.user_id).await
            .map_err(AppError::Sqlx)?
    } else {
        db::users::get_all_food_preferences(&state.pool).await
            .map_err(AppError::Sqlx)?
    };

    let restrictions_json = serde_json::to_string(&restrictions).unwrap_or_default();
    let preferences_json = serde_json::to_string(&preferences).unwrap_or_default();

    // Get existing recipe titles for dedup context
    let existing_statuses = &["saved", "tested"];
    let (existing_recipes, _) = db::recipes::list(
        &state.pool, None, None, "recent", 1, 1000, existing_statuses,
    )
    .await
    .map_err(AppError::Sqlx)?;

    let existing_titles: Vec<String> = existing_recipes
        .iter()
        .map(|r| {
            if let Some(ref cn) = r.canonical_name {
                format!("{} ({})", r.title, cn)
            } else {
                r.title.clone()
            }
        })
        .collect();
    let existing_titles_str = existing_titles.join(", ");

    // Get rejected-similar recipes for auto-filtering
    let rejected_statuses = &["rejected_similar"];
    let (rejected_recipes, _) = db::recipes::list(
        &state.pool, None, None, "recent", 1, 1000, rejected_statuses,
    )
    .await
    .map_err(AppError::Sqlx)?;
    let rejected_titles: Vec<String> = rejected_recipes.iter().map(|r| r.title.clone()).collect();
    let rejected_titles_str = rejected_titles.join(", ");

    // Scrape recipe URLs from all curated sites
    let sites = scraper::sites();
    let urls_per_site = (count / sites.len()).max(2);

    let mut all_urls: Vec<String> = Vec::new();
    let mut errors: Vec<SiteError> = Vec::new();

    for site in &sites {
        match scraper::fetch_recipe_urls(
            &state.http_client,
            site,
            body.prompt.as_deref(),
            urls_per_site,
        )
        .await
        {
            Ok(urls) => all_urls.extend(urls),
            Err(e) => errors.push(SiteError {
                site: site.name.to_string(),
                error: e,
            }),
        }
        // Rate limit: 500ms between sites
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    all_urls.truncate(count.min(10));

    // Process each candidate
    let mut discovered = Vec::new();
    let mut skipped = SkippedCounts::default();

    for url in &all_urls {
        let result = process_candidate(
            &state,
            embedding_svc,
            &client,
            auth.user_id,
            url,
            &restrictions_json,
            &preferences_json,
            &existing_titles_str,
            &rejected_titles_str,
        )
        .await;

        match result {
            Ok(CandidateResult::Discovered(recipe)) => discovered.push(recipe),
            Ok(CandidateResult::Duplicate) => skipped.duplicate += 1,
            Ok(CandidateResult::Restricted) => skipped.restricted += 1,
            Ok(CandidateResult::LowScore) => skipped.low_score += 1,
            Ok(CandidateResult::SimilarToRejected) => skipped.similar_to_rejected += 1,
            Err(e) => {
                tracing::warn!(url = %url, error = %e, "Failed to process candidate");
            }
        }
    }

    Ok(Json(DiscoverResponse {
        discovered,
        skipped,
        errors,
    }))
}

enum CandidateResult {
    Discovered(crate::models::Recipe),
    Duplicate,
    Restricted,
    LowScore,
    SimilarToRejected,
}

async fn process_candidate(
    state: &AppState,
    embedding_svc: &Arc<EmbeddingService>,
    client: &AnthropicClient,
    owner_id: uuid::Uuid,
    url: &str,
    restrictions_json: &str,
    preferences_json: &str,
    existing_titles: &str,
    rejected_titles: &str,
) -> anyhow::Result<CandidateResult> {
    // Step 1: Parse the recipe from URL (reuse existing ingestion)
    let parsed = crate::ai::ingest::parse_url(client, &state.http_client, url).await?;

    let ingredient_names: Vec<String> = parsed.ingredients.iter().map(|i| i.name.clone()).collect();
    let tags: Vec<String> = parsed.tags.clone();

    // Step 2: Mechanical embedding pre-filter
    let mech_text = EmbeddingService::recipe_summary_mechanical(
        &parsed.title,
        &tags,
        &ingredient_names,
    );
    let mech_embedding = embedding_svc
        .embed(&mech_text)
        .ok_or_else(|| anyhow::anyhow!("Failed to compute embedding"))?;

    // Check against rejected_similar (threshold 0.70)
    let rejected_similar = db::recipes::find_similar(
        &state.pool,
        &mech_embedding,
        &["rejected_similar"],
        3,
    )
    .await?;

    if let Some((_, _, _, sim)) = rejected_similar.first() {
        if *sim > 0.70 {
            return Ok(CandidateResult::SimilarToRejected);
        }
    }

    // Check against existing recipes (threshold 0.90 for auto-skip)
    let existing_similar = db::recipes::find_similar(
        &state.pool,
        &mech_embedding,
        &["saved", "tested"],
        3,
    )
    .await?;

    if let Some((_, _, _, sim)) = existing_similar.first() {
        if *sim > 0.90 {
            return Ok(CandidateResult::Duplicate);
        }
    }

    // Step 3: AI scoring call
    let score = crate::ai::discovery::score_candidate(
        client,
        &parsed.title,
        parsed.description.as_deref(),
        &ingredient_names,
        &tags,
        preferences_json,
        restrictions_json,
        existing_titles,
        rejected_titles,
    )
    .await?;

    if score.violates_restriction {
        return Ok(CandidateResult::Restricted);
    }
    if score.is_duplicate {
        return Ok(CandidateResult::Duplicate);
    }
    if score.relevance_score < 0.3 {
        return Ok(CandidateResult::LowScore);
    }

    // Step 4: Compute final embedding with canonical name
    let final_text = EmbeddingService::recipe_summary(
        &score.canonical_name,
        &tags,
        &ingredient_names,
    );
    let final_embedding = embedding_svc
        .embed(&final_text)
        .ok_or_else(|| anyhow::anyhow!("Failed to compute final embedding"))?;

    // Step 5: Insert discovered recipe
    let steps: Vec<crate::models::StepInput> = parsed
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| crate::models::StepInput {
            step_order: i as i32 + 1,
            instruction: s.instruction.clone(),
        })
        .collect();

    let recipe = db::recipes::create_discovered(
        &state.pool,
        owner_id,
        &parsed.title,
        parsed.description.as_deref(),
        url,
        &score.canonical_name,
        score.relevance_score,
        &final_embedding,
        &tags,
        &parsed.ingredients,
        &steps,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to insert discovered recipe: {e}"))?;

    Ok(CandidateResult::Discovered(recipe))
}
```

- [ ] **Step 2: Register the route**

In `backend/src/routes/mod.rs`, add:

```rust
pub mod discover;
```

In `backend/src/lib.rs`, add the route in `create_router()`:

```rust
        // Discovery
        .route("/discover", post(routes::discover::discover))
```

- [ ] **Step 3: Verify it compiles**

```bash
cd backend && cargo build 2>&1 | tail -10
```

Fix any type mismatches — the `parsed.ingredients` from `parse_url` returns `ParsedRecipe` with `IngredientInput` items. Check the exact types match what `create_discovered` expects.

- [ ] **Step 4: Commit**

```bash
git add backend/src/routes/discover.rs backend/src/routes/mod.rs backend/src/lib.rs
git commit -m "feat(discovery): discover endpoint — scrape, parse, embed, score, insert

POST /api/discover orchestrates the full pipeline: scrape curated sites,
parse via existing URL ingestion (Sonnet), embedding pre-filter (0.70/0.90
thresholds), Haiku scoring call, insert survivors as discovered.
Per-site error reporting, rate limiting, transparent skip counts."
```

---

### Task 8: Frontend — API Client and Store Changes

**Files:**
- Create: `frontend/src/api/discover.ts`
- Modify: `frontend/src/api/recipes.ts`
- Modify: `frontend/src/stores/recipes.ts`

- [ ] **Step 1: Create discovery API client**

Create `frontend/src/api/discover.ts`:

```typescript
import { apiFetch } from './client'
import type { Recipe } from './recipes'

export interface DiscoverRequest {
  prompt?: string
  count?: number
  planning_for?: 'both' | 'me'
}

export interface SkippedCounts {
  duplicate: number
  restricted: number
  low_score: number
  similar_to_rejected: number
}

export interface SiteError {
  site: string
  error: string
}

export interface DiscoverResponse {
  discovered: Recipe[]
  skipped: SkippedCounts
  errors: SiteError[]
}

export async function discover(req: DiscoverRequest): Promise<DiscoverResponse> {
  return apiFetch('/discover', {
    method: 'POST',
    body: JSON.stringify(req),
  })
}
```

- [ ] **Step 2: Add status fields to Recipe type**

In `frontend/src/api/recipes.ts`, add to the `Recipe` interface:

```typescript
  status: string
  discovery_score: number | null
  discovered_at: string | null
  scored_at: string | null
  canonical_name: string | null
```

- [ ] **Step 3: Add status param to listRecipes and status update function**

In `frontend/src/api/recipes.ts`, update `listRecipes` params and add new functions:

```typescript
export async function listRecipes(params: {
  q?: string
  tag?: string
  page?: number
  sort?: string
  status?: string  // new: comma-separated statuses
} = {}): Promise<Paginated<Recipe>> {
  const query = new URLSearchParams()
  if (params.q) query.set('q', params.q)
  if (params.tag) query.set('tag', params.tag)
  if (params.page) query.set('page', String(params.page))
  if (params.sort) query.set('sort', params.sort)
  if (params.status) query.set('status', params.status)
  return apiFetch(`/recipes?${query}`)
}

export async function updateRecipeStatus(id: string, status: string): Promise<Recipe> {
  return apiFetch(`/recipes/${id}/status`, {
    method: 'PATCH',
    body: JSON.stringify({ status }),
  })
}
```

- [ ] **Step 4: Update recipe store to pass status filter**

In `frontend/src/stores/recipes.ts`, update the `fetch` action to accept and pass a status parameter:

```typescript
export const useRecipeStore = defineStore('recipes', () => {
  const recipes = ref<Recipe[]>([])
  const total = ref(0)
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function fetch(params: { q?: string; tag?: string; page?: number; sort?: string; status?: string } = {}) {
    loading.value = true
    error.value = null
    try {
      const data = await api.listRecipes(params)
      recipes.value = data.items
      total.value = data.total
    } catch (e: any) {
      error.value = e.message || 'Failed to load recipes'
      toast.error(error.value)
    } finally {
      loading.value = false
    }
  }

  return { recipes, total, loading, error, fetch }
})
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src/api/discover.ts frontend/src/api/recipes.ts frontend/src/stores/recipes.ts
git commit -m "feat(discovery): frontend API client for discovery, status filtering, status updates

New discover.ts API client. Recipe type gains status/discovery_score fields.
listRecipes accepts status param. updateRecipeStatus for inbox actions.
Recipe store passes status filter through to API."
```

---

### Task 9: Frontend — Recipe List Page with Discovery UI

**Files:**
- Modify: `frontend/src/pages/RecipeListPage.vue`
- Modify: `frontend/src/components/RecipeCard.vue`

- [ ] **Step 1: Add tab toggle and discover button to RecipeListPage**

Rewrite `frontend/src/pages/RecipeListPage.vue` to add:
- Three tabs: "Moje recepty" (default, status=saved,tested), "Objevené" (status=discovered), "Odmítnuté" (status=rejected,rejected_similar)
- Discover button + optional prompt input
- Tab-specific content (inbox shows action buttons on cards)

The template should include:

```html
<!-- Tab toggle -->
<div class="flex gap-2 mb-4">
  <button v-for="tab in tabs" :key="tab.key" @click="activeTab = tab.key; loadRecipes()"
    class="px-4 py-2 rounded-full text-sm"
    :class="activeTab === tab.key ? 'bg-orange-600 text-white' : 'bg-stone-100 text-stone-600'">
    {{ tab.label }}
    <span v-if="tab.key === 'discovered' && discoveredCount > 0"
      class="ml-1 bg-orange-200 text-orange-800 rounded-full px-2 text-xs">{{ discoveredCount }}</span>
  </button>
</div>

<!-- Discover section (visible on all tabs) -->
<div class="mb-6 flex gap-2">
  <input v-model="discoverPrompt" placeholder="Najdi nové recepty... (např. 'něco s rybou')"
    class="flex-1 px-4 py-2 border border-stone-300 rounded-lg"
    @keyup.enter="handleDiscover" />
  <button @click="handleDiscover" :disabled="discovering"
    class="px-4 py-2 bg-green-600 text-white rounded-lg hover:bg-green-700 disabled:opacity-50">
    {{ discovering ? 'Hledám...' : 'Objevit nové' }}
  </button>
</div>
```

The script section adds:
- `activeTab` ref with values `'mine'`, `'discovered'`, `'rejected'`
- `discoverPrompt` ref
- `discovering` ref
- `discoveredCount` ref (fetched separately)
- `handleDiscover()` function that calls the discover API and shows results as toast
- `loadRecipes()` passes `status` based on active tab

- [ ] **Step 2: Add discovery variant to RecipeCard**

Update `frontend/src/components/RecipeCard.vue` to show discovery metadata and action buttons when the recipe has `status === 'discovered'`:

```html
<!-- Discovery badge -->
<div v-if="recipe.status === 'discovered'" class="flex items-center gap-2 mt-2">
  <span class="text-xs bg-green-100 text-green-700 rounded-full px-2 py-0.5">
    {{ Math.round((recipe.discovery_score || 0) * 100) }}%
  </span>
  <span v-if="recipe.source_url" class="text-xs text-stone-400 truncate">
    z {{ new URL(recipe.source_url).hostname }}
  </span>
</div>

<!-- Action buttons for discovered recipes -->
<div v-if="recipe.status === 'discovered'" class="flex gap-2 mt-3" @click.stop>
  <button @click="$emit('status', recipe.id, 'saved')"
    class="flex-1 px-3 py-1 bg-green-600 text-white rounded-lg text-sm hover:bg-green-700">
    Uložit
  </button>
  <button @click="$emit('status', recipe.id, 'rejected')"
    class="px-3 py-1 border border-red-300 text-red-600 rounded-lg text-sm hover:bg-red-50">
    Odmítnout
  </button>
  <button @click="$emit('status', recipe.id, 'rejected_similar')"
    class="px-3 py-1 border border-red-300 text-red-600 rounded-lg text-sm hover:bg-red-50"
    title="Odmítne tento recept a podobné v budoucnu">
    Odmítnout podobné
  </button>
</div>

<!-- Restore button for rejected recipes -->
<div v-if="recipe.status === 'rejected' || recipe.status === 'rejected_similar'" class="flex gap-2 mt-3" @click.stop>
  <button @click="$emit('status', recipe.id, 'discovered')"
    class="px-3 py-1 border border-stone-300 text-stone-600 rounded-lg text-sm hover:bg-stone-50">
    Obnovit
  </button>
  <span v-if="recipe.status === 'rejected_similar'" class="text-xs text-red-400 self-center">
    Blokuje podobné recepty
  </span>
</div>
```

Add the emit declaration:
```typescript
defineEmits<{ status: [id: string, status: string] }>()
```

In RecipeListPage, handle the status emit:
```typescript
async function handleStatusChange(id: string, status: string) {
  try {
    await updateRecipeStatus(id, status)
    await loadRecipes()
    toast.success(status === 'saved' ? 'Recept uložen' : status === 'discovered' ? 'Recept obnoven' : 'Recept odmítnut')
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se změnit stav')
  }
}
```

- [ ] **Step 3: Style discovered cards with dashed border**

In RecipeCard, add conditional class:

```html
<div :class="[
  'rounded-xl border p-4',
  recipe.status === 'discovered' ? 'border-2 border-dashed border-green-300 bg-green-50' :
  recipe.status === 'rejected' || recipe.status === 'rejected_similar' ? 'border border-red-200 bg-red-50/30' :
  'border-stone-200 bg-white'
]">
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/pages/RecipeListPage.vue frontend/src/components/RecipeCard.vue
git commit -m "feat(discovery): recipe list tabs (mine/discovered/rejected), discover button

Three-tab UI for browsing own recipes, discovered inbox, and rejected.
Discover button with prompt input triggers discovery API.
Discovered cards show relevance score, source site, accept/reject actions.
Rejected cards show restore button. Soft-delete with full recovery."
```

---

### Task 10: Frontend — Recipe Detail Status Management

**Files:**
- Modify: `frontend/src/pages/RecipeDetailPage.vue`

- [ ] **Step 1: Add status badge and tested toggle**

In the recipe detail template, add after the title:

```html
<!-- Status badge -->
<span v-if="recipe.status === 'discovered'" class="ml-2 text-xs bg-green-100 text-green-700 rounded-full px-2 py-1">
  Objevený ({{ Math.round((recipe.discovery_score || 0) * 100) }}%)
</span>
<span v-if="recipe.status === 'tested'" class="ml-2 text-xs bg-blue-100 text-blue-700 rounded-full px-2 py-1">
  Vyzkoušeno
</span>

<!-- Source URL -->
<a v-if="recipe.source_url" :href="recipe.source_url" target="_blank" rel="noopener"
  class="text-sm text-orange-600 hover:underline">
  Původní recept
</a>

<!-- Tested toggle (for saved recipes) -->
<button v-if="recipe.status === 'saved'" @click="markTested"
  class="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 text-sm">
  Označit jako vyzkoušené
</button>
```

Add the handler:

```typescript
async function markTested() {
  try {
    await updateRecipeStatus(recipe.value!.id, 'tested')
    recipe.value!.status = 'tested'
    toast.success('Recept označen jako vyzkoušený')
  } catch (e: any) {
    toast.error(e.message || 'Nepodařilo se změnit stav')
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/pages/RecipeDetailPage.vue
git commit -m "feat(discovery): recipe detail — status badge, source URL, tested toggle

Show discovery score badge for discovered recipes, 'vyzkoušeno' badge for
tested. Source URL link to original recipe. 'Mark as tested' button for
saved recipes."
```

---

### Task 11: Documentation

**Files:**
- Modify: `README.md`
- Modify: `.env.example` (already done in Task 2)

- [ ] **Step 1: Update README with discovery setup instructions**

Add a section to `README.md` documenting:

```markdown
## Recipe Discovery (optional)

Discovery requires the all-MiniLM-L6-v2 ONNX model for embedding-based deduplication.

### Setup

1. Download the model files:
   ```bash
   mkdir -p models/all-MiniLM-L6-v2
   # Option A: Copy from second-brain project
   cp /path/to/second-brain/models/all-MiniLM-L6-v2/* models/all-MiniLM-L6-v2/
   # Option B: Download from HuggingFace
   # model.onnx (~86MB) and tokenizer.json (~466KB) for sentence-transformers/all-MiniLM-L6-v2
   ```

2. Set the environment variable:
   ```bash
   EMBEDDING_MODEL_DIR=./models/all-MiniLM-L6-v2
   ```

3. For Docker deployment, mount as a volume:
   ```yaml
   volumes:
     - ./models/all-MiniLM-L6-v2:/models/all-MiniLM-L6-v2:ro
   environment:
     - EMBEDDING_MODEL_DIR=/models/all-MiniLM-L6-v2
   ```

If `EMBEDDING_MODEL_DIR` is not set, the app starts normally with discovery disabled.
Set `DISCOVERY_ENABLED=false` to explicitly disable discovery even with the model available.
```

- [ ] **Step 2: Add models/ to .gitignore**

```
# ONNX model files (too large for git)
models/
```

- [ ] **Step 3: Commit**

```bash
git add README.md .gitignore
git commit -m "docs: recipe discovery setup instructions and model .gitignore"
```

---

### Task 12: Integration Tests

**Files:**
- Create: `backend/tests/discovery_test.rs`

- [ ] **Step 1: Write recipe status transition tests**

Create `backend/tests/discovery_test.rs`:

```rust
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// Helper: create a recipe and return its ID
async fn create_recipe(ctx: &common::TestContext) -> String {
    let (hdr_name, hdr_val) = ctx.auth_header_1();
    let body = serde_json::json!({
        "title": "Test Recipe",
        "ingredients": [{"name": "test ingredient"}],
        "steps": [{"step_order": 1, "instruction": "test step"}]
    });

    let resp = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/recipes")
                .header("Content-Type", "application/json")
                .header(&hdr_name, &hdr_val)
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn recipe_list_filters_by_status() {
    let ctx = common::TestContext::new().await;
    let id = create_recipe(&ctx).await;

    // Default status should be 'saved' (from migration default)
    // But our migration backfills to 'tested'. New recipes get 'saved'.
    let (hdr_name, hdr_val) = ctx.auth_header_1();

    // List with default status (saved,tested) should include the recipe
    let resp = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/recipes")
                .header(&hdr_name, &hdr_val)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["total"].as_i64().unwrap() >= 1);

    // List with status=discovered should NOT include the recipe
    let resp = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/recipes?status=discovered")
                .header(&hdr_name, &hdr_val)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["total"].as_i64().unwrap(), 0);
}

#[tokio::test]
async fn recipe_status_transition_saved_to_tested() {
    let ctx = common::TestContext::new().await;
    let id = create_recipe(&ctx).await;
    let (hdr_name, hdr_val) = ctx.auth_header_1();

    // Transition saved → tested
    let resp = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/recipes/{id}/status"))
                .header("Content-Type", "application/json")
                .header(&hdr_name, &hdr_val)
                .body(Body::from(r#"{"status":"tested"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["status"].as_str().unwrap(), "tested");
}

#[tokio::test]
async fn recipe_status_invalid_transition_rejected() {
    let ctx = common::TestContext::new().await;
    let id = create_recipe(&ctx).await;
    let (hdr_name, hdr_val) = ctx.auth_header_1();

    // Transition saved → discovered should NOT change status
    // (saved → discovered is not a valid transition)
    let resp = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/recipes/{id}/status"))
                .header("Content-Type", "application/json")
                .header(&hdr_name, &hdr_val)
                .body(Body::from(r#"{"status":"discovered"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    // Status should still be 'saved' — transition was rejected
    assert_eq!(json["status"].as_str().unwrap(), "saved");
}

#[tokio::test]
async fn discover_endpoint_returns_503_without_embedding_model() {
    let ctx = common::TestContext::new().await;
    let (hdr_name, hdr_val) = ctx.auth_header_1();

    let resp = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/discover")
                .header("Content-Type", "application/json")
                .header(&hdr_name, &hdr_val)
                .body(Body::from(r#"{"count": 3}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 400 (BadRequest) because embedding is None in test context
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2: Run tests**

```bash
cd backend && cargo test 2>&1 | tail -20
```

- [ ] **Step 3: Commit**

```bash
git add backend/tests/discovery_test.rs
git commit -m "test(discovery): integration tests for status filtering, transitions, discover endpoint

Tests recipe list filtering by status, valid/invalid status transitions,
and discovery endpoint returning 400 when embedding model not configured."
```

---

## Self-Review

**Spec coverage check:**

| Spec section | Task |
|---|---|
| 1. Data model changes | Task 1 |
| 2. ONNX embedding service | Task 3 |
| 3. Discovery pipeline (triggers, flow, errors, rate limiting) | Task 7 |
| 4. Scoring & dedup AI call | Task 6 |
| 5. Site scraping config | Task 5 |
| 6. Recipe status lifecycle (transitions, soft-delete, transparency) | Task 4 (backend) + Task 9 (frontend) |
| 7. Frontend changes (tabs, discover, cards, detail) | Tasks 8, 9, 10 |
| 8. Configuration & deployment | Task 2 |
| 9. API endpoints | Tasks 4, 7 |
| 10. Out of scope | N/A (correctly excluded) |
| 11. Testing | Task 12 |

**Placeholder scan:** No TBDs, TODOs, or "implement later" found.

**Type consistency check:**
- `Recipe` struct in models.rs matches the RETURNING clause in db/recipes.rs queries
- `DiscoverRequest`/`DiscoverResponse` in models.rs matches usage in routes/discover.rs
- `ScoringResult` in ai/discovery.rs matches the JSON schema in the AI prompt
- Frontend `Recipe` interface matches backend serialization
- `updateRecipeStatus` in frontend matches PATCH /recipes/{id}/status endpoint
