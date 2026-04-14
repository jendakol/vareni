# Recipe Discovery — Design Spec

## Goal

Automatically discover new recipes from curated Czech recipe websites, deduplicate them against the existing recipe book using embeddings + AI, let users review and accept/reject candidates, and integrate discoveries into meal planning.

## Context

The cooking app currently supports three manual recipe ingestion methods (text, photo, URL) and AI-powered meal planning that picks from the user's existing recipe book. The recipe book is small (~25 recipes) and grows slowly. The meal planner frequently suggests the same meals. This feature introduces proactive recipe discovery to expand the book with minimal user effort.

## Architecture

Two-phase pipeline: **scrape + parse** (reuses existing URL ingestion via Claude Sonnet) then **score + dedup** (new Haiku call that normalizes titles, checks restrictions, detects duplicates, and scores relevance). Embedding-based pre-filtering (all-MiniLM-L6-v2 via ONNX, 384 dimensions) provides cheap dedup before the AI call. Recipes enter a `discovered` status for user review before joining the main recipe book.

## Tech Stack additions

- `ort` 2.0.0-rc.12 + `tokenizers` 0.22 + `ndarray` 0.17 (ONNX embedding, same versions as Second Brain)
- `pgvector` crate for sqlx vector column support
- all-MiniLM-L6-v2 model files (~90MB) mounted as a Docker volume (NOT baked into the image)

---

## 1. Data Model Changes

### New columns on `recipes` table

```sql
-- Migration: 004_recipe_discovery.sql

-- Recipe lifecycle status
ALTER TABLE recipes ADD COLUMN status TEXT NOT NULL DEFAULT 'saved';
-- Values: 'discovered', 'saved', 'tested', 'rejected', 'rejected_similar'

-- Embedding for dedup (all-MiniLM-L6-v2, 384 dimensions)
ALTER TABLE recipes ADD COLUMN embedding vector(384);

-- AI-assigned relevance score (0.0-1.0), NULL for non-discovered
ALTER TABLE recipes ADD COLUMN discovery_score REAL;

-- When this recipe was discovered, NULL for manual additions
ALTER TABLE recipes ADD COLUMN discovered_at TIMESTAMPTZ;

-- When the discovery_score was computed (for staleness detection)
ALTER TABLE recipes ADD COLUMN scored_at TIMESTAMPTZ;

-- LLM-normalized canonical dish name for dedup
ALTER TABLE recipes ADD COLUMN canonical_name TEXT;

-- Backfill: all existing recipes are "tested" (already cooked)
UPDATE recipes SET status = 'tested';

-- Indexes
CREATE INDEX idx_recipes_status ON recipes (status);
CREATE INDEX idx_recipes_embedding ON recipes USING hnsw (embedding vector_cosine_ops);
```

### Impact on existing queries

- **Recipe list (browsing):** Add `WHERE status IN ('saved', 'tested')` — discovered/rejected recipes excluded from normal browsing.
- **Recipe detail:** No filter — any recipe is viewable by ID (for inbox previews).
- **Meal plan suggest:** `WHERE status IN ('saved', 'tested')` — only user-accepted recipes. Discovered recipes are NOT suggested by the planner.
- **Inbox view (new):** `WHERE status = 'discovered' ORDER BY discovery_score DESC`.
- **Rejected view (new):** `WHERE status IN ('rejected', 'rejected_similar') ORDER BY updated_at DESC` — for recovery from accidental rejections.

### sqlx considerations

- Add `pgvector` crate to Cargo.toml.
- The `embedding` column requires `pgvector::Vector` type in Rust models.
- Run `cargo sqlx prepare` after migration to regenerate `.sqlx/` offline metadata.
- All existing `SELECT r.*` queries must be updated to include new columns or use explicit column lists.

---

## 2. ONNX Embedding Service

### Model deployment

- Model files (`model.onnx` ~86MB, `tokenizer.json` ~466KB) are mounted as a Docker volume, NOT embedded in the image.
- Path configured via `EMBEDDING_MODEL_DIR` environment variable.
- **Startup behavior:** If `EMBEDDING_MODEL_DIR` is not set or the directory doesn't contain `model.onnx` and `tokenizer.json`, the server logs a clear warning and starts with discovery DISABLED. All other features work normally. The `/api/discover` endpoint returns 503 with `{"error": "Discovery is unavailable: embedding model not configured"}`.

### Embedding generation

The embedding text is constructed as:

```
{canonical_name}. {canonical_name}. Kategorie: {tags}. Obsahuje: {top_5_filtered_ingredients}.
```

Where:
- `canonical_name` is the LLM-normalized Czech dish name (e.g. "kuře na paprice" instead of "Jednoduché kuře na paprice s rýží").
- Title is repeated 2x to give it more weight in mean pooling.
- Tags are included as-is.
- Ingredients are filtered to remove stop-ingredients (sůl, pepř, olej, česnek, cibule, voda, máslo) and limited to the first 5.

### Embedding generation for existing recipes

Existing recipes don't have `canonical_name` yet. The migration backfill:
1. For each existing recipe, call Haiku to generate `canonical_name`.
2. Compute embedding from the canonical name + tags + ingredients.
3. Store both in the database.

This is a one-time migration task that can be run as a CLI command (`cargo run --bin backfill-embeddings`) or an admin endpoint.

### Benchmark results

Validated via `tools/embedding-bench/` with 25 real recipes + 12 synthetic duplicates + 10 scraped web recipes:

| Pair type | Score range | Threshold action |
|---|---|---|
| True duplicates (same dish, different wording) | 0.78 - 0.96 | Auto-flag for AI confirmation |
| Same category, different dish | 0.55 - 0.65 | Safe to skip |
| Clearly different dishes | 0.25 - 0.55 | Safe to skip |

**Pre-filter threshold: 0.70** — candidates above this are flagged for AI dedup confirmation. The benchmark tool remains in the repo for future threshold tuning.

---

## 3. Discovery Pipeline

### Triggers (v1)

**Trigger 1: On-demand** — User clicks "Objevit" (Discover) button on the recipe list page with an optional text prompt.

**Trigger 2: Inbox browsing** — User reviews previously discovered recipes and accepts/rejects them.

**NOT in v1:** Inline discovery during meal planning (deferred — the planner integration is underspecified and adds complexity to the plan suggest flow).

**NOT in v1:** Background/scheduled crawling (deferred — on-demand is sufficient for a 2-user household).

### On-demand discovery flow

```
User clicks "Objevit" with optional prompt ("něco s rybou")
  │
  ▼
POST /api/discover { prompt: "...", count: 5, planning_for: "both" | "me" }
  │
  ▼
Backend: for each curated site:
  1. Fetch listing/search page → extract recipe URLs (per-site CSS selectors)
  2. Select up to `count` URLs (random if no prompt, AI-selected if prompt provided)
  │
  ▼
For each candidate URL (max 10 total):
  3. Fetch recipe page
  4. Parse via existing ai::ingest::parse_url (Claude Sonnet) → recipe data
  5. Compute embedding pre-filter (BEFORE the Haiku call, to save API costs):
     a. Build a quick mechanical embedding from raw title (repeated 2x) + tags + top 5 filtered
        ingredients. This does NOT use canonical_name (we don't have it yet — Haiku hasn't run).
     b. Check cosine distance against all existing recipe embeddings (which DO use canonical_name)
     c. Check cosine distance against all rejected_similar recipe embeddings
     d. If distance > 0.90 against existing → skip (almost certain duplicate, save the Haiku call)
     e. If distance > 0.70 against rejected_similar → skip (similar to something user rejected)
     Note: The 0.90 threshold is deliberately higher than the 0.70 dedup threshold because the
     mechanical embedding (no canonical name) is less accurate. We only skip the Haiku call for
     very high confidence matches. Borderline cases (0.70-0.90) still get the Haiku call.
  6. Call Haiku scoring endpoint (see section 4)
  7. Apply decision logic (see section 4)
  8. Insert survivors as status='discovered'
  │
  ▼
Return to frontend:
{
  "discovered": [...recipes...],
  "skipped": { "duplicate": 2, "restricted": 1, "low_score": 1 },
  "errors": [
    { "site": "kuchynelidlu.cz", "error": "HTTP 403 Forbidden" }
  ]
}
```

### Error handling and transparency

- **Per-site errors** are reported individually in the response. If one site is down or blocks us, the others still work.
- **AI failures** (Anthropic API down): discovery returns 503 with a clear error message. The rest of the app is unaffected.
- **Scraping failures** (HTML changed, no recipes found): reported as site-level errors with descriptive messages (e.g. "No recipe links found on fresh.iprima.cz — the site layout may have changed").
- **Timeouts:** Per-fetch timeout of 10 seconds (existing reqwest config). Overall request timeout managed by the frontend (show progress, allow cancel).

### Rate limiting

- Max 10 recipe fetches per discover request.
- No concurrent requests to the same site.
- 500ms delay between fetches to the same site.
- No background crawling in v1.

---

## 4. Scoring & Dedup AI Call

### Single Haiku call per candidate

```
You are evaluating a recipe candidate for a household meal planning app.

Candidate recipe:
Title: {title}
Description: {description}
Ingredients: {ingredients}
Tags: {tags}

User's food preferences: {preferences_json}
User's dietary restrictions: {restrictions_json}

Existing recipes in the book: {existing_titles_with_canonical_names}
Previously rejected-similar recipes: {rejected_titles}

Tasks:
1. CANONICAL NAME: Reduce the recipe title to a short canonical Czech dish name
   (e.g. "kuře na paprice", "mac and cheese", "špenátový salát s fetou")
2. RESTRICTION CHECK: Does this recipe violate any dietary restriction? (yes/no, which one)
3. DUPLICATE CHECK: Is this essentially the same dish as any existing recipe?
   Consider the canonical name and ingredients, not just the title.
   (yes/no, which existing recipe)
4. RELEVANCE SCORE: Rate 0.0-1.0 how well this matches the user's food preferences.
   1.0 = perfect match, 0.0 = completely irrelevant.

Return ONLY valid JSON:
{
  "canonical_name": "string",
  "violates_restriction": false,
  "restriction_violated": null,
  "is_duplicate": false,
  "duplicate_of": null,
  "relevance_score": 0.8
}
```

### Decision logic

Applied in order:
1. `violates_restriction == true` → auto-reject, do not insert.
2. `is_duplicate == true` → auto-reject, do not insert.
3. `relevance_score < 0.3` → auto-reject, do not insert.
4. Otherwise → insert as `status = 'discovered'`.

After insertion:
- Compute final embedding using `canonical_name` (from LLM) + tags + filtered ingredients.
- Store `canonical_name`, `embedding`, `discovery_score`, `discovered_at`, `scored_at`.

### Preference staleness

- `scored_at` records when the score was computed.
- When user changes preferences or restrictions, discovered recipes with `scored_at` older than the change are marked stale.
- Stale recipes show a visual indicator in the inbox: "Score may be outdated — re-score?"
- Re-scoring: re-run the Haiku call for stale recipes on demand (button in inbox).

---

## 5. Site Scraping Configuration

### Per-site config

```rust
struct SiteConfig {
    name: &'static str,
    base_url: &'static str,
    listing_path: &'static str,
    search_path: Option<&'static str>,  // e.g. "/vyhledavani?q={query}"
    link_selector: &'static str,        // CSS selector for recipe links
}
```

### v1 sites

| Site | Listing | Search | Link selector |
|---|---|---|---|
| fresh.iprima.cz | /recepty | N/A | `a[href*="/recepty/"]` (to be verified) |
| kuchynelidlu.cz | /recepty | /vyhledavani?q={query} | `a[href*="/recept/"]` |
| receptyodanicky.cz | / | N/A | `a[href*=".cz/"]` (to be verified) |

Link selectors will be verified during implementation by inspecting actual HTML. The config is a Rust struct (not a database table) — adding a new site requires a code change and redeploy.

### Discovery with prompt vs without

- **With prompt:** If site supports search, use `search_path` with the prompt as query. Otherwise, fetch the listing page and let the Haiku scoring call filter by relevance.
- **Without prompt:** Fetch the listing page, pick random recipe links.

---

## 6. Recipe Status Lifecycle

```
Manual entry (text/photo/URL) ──→ saved ──→ tested
                                    ▲
Discovery pipeline ──→ discovered ──┤
                          │         │
                          ├──→ rejected ──→ (recoverable)
                          │
                          └──→ rejected_similar ──→ (recoverable, used for auto-filtering)
```

### Status transitions

| From | To | Trigger |
|---|---|---|
| discovered | saved | User clicks "Uložit" in inbox |
| discovered | rejected | User clicks "Odmítnout" in inbox |
| discovered | rejected_similar | User clicks "Odmítnout podobné" in inbox |
| saved | tested | User clicks "Vyzkoušeno" on recipe detail |
| rejected | discovered | User clicks "Obnovit" in rejected view (undo) |
| rejected_similar | discovered | User clicks "Obnovit" in rejected view (undo) |

### Soft-delete and recovery

`rejected` and `rejected_similar` are NOT terminal states. They are recoverable:

- A "Odmítnuté" (Rejected) tab in the inbox shows rejected recipes.
- Each rejected recipe has an "Obnovit" (Restore) button that moves it back to `discovered`.
- `rejected_similar` recipes continue to suppress similar future candidates until restored.
- No automatic permanent deletion. Rejected recipes stay in the database indefinitely (the table is small for a household app).

### Auto-rejection transparency

When the discovery pipeline auto-rejects candidates (due to embedding match against `rejected_similar`), the response includes:

```json
{
  "skipped": {
    "duplicate": 2,
    "restricted": 1,
    "low_score": 1,
    "similar_to_rejected": 3
  }
}
```

The user sees: "3 recipes skipped (similar to recipes you previously rejected)." This gives visibility into what the auto-filter is doing.

---

## 7. Frontend Changes

### Recipe list page (`RecipeListPage.vue`)

- **Tab toggle:** "Moje recepty" (saved+tested, default) | "Objevené" ({count} badge) | "Odmítnuté"
- **Discover button + input:** "Objevit nové" button with optional text input, triggers `POST /api/discover`
- **Loading state:** "Hledám nové recepty..." with per-site progress if available. Cancelable.
- **Results:** Toast notification: "Nalezeno 5 nových receptů" or "Nenalezeno nic nového"

### Discovered recipe card

- Same visual as regular recipe card but with a dashed border (similar to plan suggestions)
- Relevance score badge (e.g. "87%")
- Source site label (e.g. "z kuchynelidlu.cz")
- Three action buttons:
  - "Uložit" (green) → status = saved
  - "Odmítnout" (red) → status = rejected
  - "Odmítnout podobné" (red, with tooltip explaining the effect) → status = rejected_similar
- Stale score indicator if `scored_at` is older than last preference change

### Rejected recipe view

- Shows rejected + rejected_similar recipes
- "Obnovit" (Restore) button per recipe
- For `rejected_similar`: indicator showing "Blokuje podobné recepty"

### Recipe detail page (`RecipeDetailPage.vue`)

- **"Vyzkoušeno" toggle:** For saved recipes, a button/toggle to mark as tested
- **Status badge:** Shows current status (discovered/saved/tested)
- **Source URL:** If `source_url` is set, show a link to the original recipe page

### Settings page

No changes needed — preferences and restrictions are already managed there. Discovery uses them via the existing API.

---

## 8. Configuration & Deployment

### Environment variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `EMBEDDING_MODEL_DIR` | No | — | Path to directory with `model.onnx` and `tokenizer.json`. If not set, discovery is disabled. |
| `DISCOVERY_ENABLED` | No | `true` | Master switch to disable discovery even if model is available. |

### Docker compose changes

```yaml
services:
  backend:
    volumes:
      - ./models/all-MiniLM-L6-v2:/models/all-MiniLM-L6-v2:ro
    environment:
      - EMBEDDING_MODEL_DIR=/models/all-MiniLM-L6-v2
```

### Startup behavior

1. Check `DISCOVERY_ENABLED` — if `false`, skip model loading, discovery endpoints return 503.
2. Check `EMBEDDING_MODEL_DIR` — if not set or files missing, log warning, discovery endpoints return 503.
3. Load ONNX model on a blocking thread (avoid async runtime deadlock).
4. Log: "ONNX embedding model loaded from {path}" or "Discovery disabled: embedding model not configured".
5. All other features (recipe CRUD, meal planning, chat, etc.) work regardless of discovery status.

### Model files

The model files are NOT committed to the git repo (too large). They are:
- Downloaded once from HuggingFace (or copied from Second Brain's `models/` directory).
- Stored on the host filesystem.
- Mounted into the container as a read-only volume.
- Documented in README with setup instructions.

---

## 9. API Endpoints

### New endpoints

| Method | Path | Description |
|---|---|---|
| POST | `/api/discover` | Trigger on-demand discovery |
| GET | `/api/recipes?status=discovered` | List discovered recipes (inbox) |
| GET | `/api/recipes?status=rejected,rejected_similar` | List rejected recipes |
| PATCH | `/api/recipes/{id}/status` | Change recipe status (save, reject, reject_similar, restore, mark_tested) |
| POST | `/api/recipes/{id}/rescore` | Re-run scoring for a stale discovered recipe |

### POST /api/discover

Request:
```json
{
  "prompt": "něco s rybou",
  "count": 5,
  "planning_for": "both"
}
```

Response:
```json
{
  "discovered": [
    {
      "id": "uuid",
      "title": "Pečená pražma s medem",
      "canonical_name": "pečená pražma",
      "discovery_score": 0.87,
      "source_url": "https://kuchynelidlu.cz/recept/pecena-prazma",
      "tags": ["ryba"],
      "description": "..."
    }
  ],
  "skipped": {
    "duplicate": 2,
    "restricted": 0,
    "low_score": 1,
    "similar_to_rejected": 0
  },
  "errors": []
}
```

### PATCH /api/recipes/{id}/status

Request:
```json
{
  "status": "saved"
}
```

Allowed transitions enforced server-side (see section 6).

---

## 10. Out of Scope (v1)

- **Background/scheduled crawling** — deferred, on-demand is sufficient.
- **Inline discovery during meal planning** — deferred, needs more design work on how the planner AI communicates "go discover" vs "use existing recipe."
- **User-configurable site list** — sites are hardcoded in a Rust config struct. Adding a new site requires a code change.
- **Social features** — no sharing of discovered recipes between users.
- **Recipe dedup across users** — both users see the same recipe book; dedup is global.

---

## 11. Testing Strategy

### Unit tests
- Embedding summary generation (template functions)
- Stop-ingredient filtering
- Status transition validation (allowed transitions matrix)
- Site config URL construction

### Integration tests
- Discovery endpoint with mocked HTTP responses (avoid hitting real sites)
- Recipe status transitions via API
- Embedding storage and cosine distance queries via pgvector
- Scoring call with mocked Anthropic API

### Manual testing
- End-to-end discovery from real sites
- Inbox accept/reject flow
- Rejected recipe recovery
- Discovery with model not configured (graceful 503)

### Benchmark tool
- `tools/embedding-bench/` remains in the repo for embedding quality validation.
- Run manually when tuning thresholds or changing summary templates.
