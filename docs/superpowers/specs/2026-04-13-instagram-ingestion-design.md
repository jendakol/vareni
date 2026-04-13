# Instagram Recipe Ingestion — Design Spec

## Goal

Enable importing recipes from Instagram reels/posts by pasting the URL in the existing "Web" tab. The backend detects Instagram URLs, extracts the caption (which typically contains ingredients and sometimes steps) from HTML meta tags, and sends it to the AI for recipe parsing.

## Architecture

Enhance the existing URL ingestion pipeline with Instagram-aware extraction. No new UI tabs or pages — the current "Web" tab handles it. The backend uses a proper User-Agent header and extracts content from `<meta>` tags instead of body text when it detects an Instagram URL.

**Tech:** Rust/Axum backend (existing `parse_url` in `backend/src/ai/ingest.rs`), Vue 3 frontend (existing `RecipeNewPage.vue`).

---

## How Instagram serves content

Instagram returns full post metadata in the initial HTML response to any request with a desktop Chrome User-Agent — no cookies or authentication required.

Available in the HTML:
- `<meta name="description">` — full caption text (ingredients, hashtags, description)
- `<meta property="og:title">` — author name + caption
- `<meta property="og:image">` — thumbnail image (640x640 JPG)

The caption is the primary data source. For reels, the video procedure is not extractable, but the caption typically lists ingredients and sometimes steps. The AI's `guessed_fields` mechanism already handles marking inferred content.

**Verified:** A plain `curl` with `User-Agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36` returns full meta tags from `instagram.com/reels/DCwqunhI7l6/` without cookies.

**Fragility note:** Instagram can change this behavior at any time. The design includes graceful fallback — see Error handling section.

---

## Backend changes

### 1. User-Agent header and timeout on HTTP client

`parse_url()` currently creates a bare `reqwest::Client::new()` per request with no User-Agent. Instagram (and potentially other sites) return different content based on UA.

**Changes:**
- Configure the `reqwest::Client` with a desktop Chrome User-Agent header.
- Set an explicit 10-second timeout (default 30s is too long for a user-facing request).
- Move client creation to `AppState` instead of per-request (pre-existing issue, good time to fix).

File: `backend/src/ai/ingest.rs` — `parse_url()` function signature changes to accept `&reqwest::Client`.
File: `backend/src/lib.rs` or `backend/src/main.rs` — create client in `AppState`.

### 2. Instagram-specific extraction

When the URL matches `instagram.com`, extract content from meta tags instead of body text.

**Detection:** Parse the URL with `url::Url`, check that `host_str()` ends with `instagram.com` (covers `www.instagram.com`, `m.instagram.com`, bare `instagram.com`). This prevents SSRF via domains like `instagram.com.evil.local`. Match paths starting with `/reel/`, `/reels/`, `/p/` (posts), or `/tv/` (legacy IGTV links).

**Extraction logic:**
1. Fetch the HTML with desktop Chrome UA and 10s timeout
2. Parse HTML with `scraper` (already a dependency)
3. Extract `<meta name="description" content="...">` — the caption
4. Extract `<meta property="og:title" content="...">` — author + title
5. **HTML entity decode** the extracted content (meta tag values contain `&amp;`, `&#x27;`, Unicode escapes)
6. **Validate** the caption is non-empty and doesn't look like a generic Instagram page (e.g., not just "Instagram" or a login page tagline)
7. **Detect login redirects**: if the response redirected to a URL containing `/accounts/login`, treat as a failed fetch
8. Build a text block for the AI: combine author, caption, and note that this is from Instagram

**If extraction fails (empty caption, login redirect, rate limit):** Return an error suggesting the user paste the caption as text instead.

File: `backend/src/ai/ingest.rs` — new `extract_instagram()` function, called from `parse_url()` when Instagram is detected.

### 3. Source URL — already exists

The `source_url` column already exists on the `recipes` table (migration 001), and `source_url: Option<String>` is already on `Recipe`, `CreateRecipeRequest`. **No migration or model changes needed.**

One gap: `UpdateRecipeRequest` does not include `source_url`, so it can't be edited after creation. This is acceptable for now — source URL is set at creation time and shouldn't need editing.

### 4. Source type decision

The `recipes` table has `CHECK (source_type IN ('manual', 'photo', 'url'))`. Instagram-sourced recipes will use `source_type = 'url'` — the `source_url` field already distinguishes Instagram from other web sources. No CHECK constraint change needed.

### 5. Thumbnail download (out of scope)

Download the `og:image` thumbnail and store it as the recipe's image. This gives visual context on the recipe list. **Deferred** — the recipe is fully functional without it.

---

## Frontend changes

### 1. Instagram URL feedback

When the user pastes an Instagram URL in the "Web" tab, show "Importuji z Instagramu..." in the processing toast (instead of generic "Stahuji recept...").

**Detection:** Check if the URL input contains `instagram.com`.

File: `frontend/src/pages/RecipeNewPage.vue` — `handleIngest()` function.

### 2. Source URL passthrough in recipe form

Pass `source_url` through the ingestion → preview → save flow:
- `RecipeNewPage.vue`: set `result.source_url = urlInput.value` on the preview result when using the URL tab
- `RecipeForm.vue`: include `source_url` from `props.initial` in the form data emitted on save

Both files need changes — `RecipeForm.vue` currently does not carry `source_url` through.

Files:
- `frontend/src/pages/RecipeNewPage.vue`
- `frontend/src/components/RecipeForm.vue`

### 3. Source URL display on recipe detail

If a recipe has a `source_url`, show it as a small link on the recipe detail page. For Instagram URLs, display as "Zdroj: Instagram" with a link icon. For other URLs, show the domain.

File: `frontend/src/pages/RecipeDetailPage.vue` — add below recipe title/description.

---

## AI prompt considerations

The Instagram caption sent to the AI parser should include context:

```
Source: Instagram post by {author}
URL: {url}

Caption:
{caption text}
```

This helps the AI understand:
- The text may be informal / emoji-heavy
- Steps may be missing (video-only) — mark as guessed
- Ingredients may be in a non-Czech language — translate as usual
- Hashtags at the end should be converted to tags

The existing `INGEST_SYSTEM` prompt already handles translation, guessing, and tag extraction. No prompt changes needed — just prepend the context to the text input.

---

## Error handling

- **Instagram returns no caption (login wall, rate limit):** Return `AppError::BadRequest("Nepodařilo se načíst recept z Instagramu. Zkuste zkopírovat popisek z Instagramu a vložit ho jako text.")` — suggests the text fallback.
- **Login redirect detected (302 to `/accounts/login`):** Same error as above.
- **Instagram URL format not recognized (not `/reel/`, `/p/`, etc.):** Fall through to standard URL extraction (existing behavior).
- **Network error / timeout fetching Instagram:** Same error handling as current URL fetch.

---

## Scope boundaries

**In scope:**
- Instagram URL detection in `parse_url()` (proper host parsing, not string contains)
- Meta tag extraction with HTML entity decoding (description, og:title)
- Desktop Chrome User-Agent + 10s timeout on HTTP client
- Move `reqwest::Client` to `AppState`
- Frontend: Instagram-specific toast, source URL passthrough (RecipeNewPage + RecipeForm), source URL display on detail page

**Out of scope (future work):**
- Thumbnail download and storage
- Instagram Stories (ephemeral, not linkable)
- Video transcription / frame extraction
- Instagram API authentication
- Browser extension or share-to-app flow
- `source_url` editing via `UpdateRecipeRequest`
