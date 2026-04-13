# Instagram Recipe Ingestion — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable importing recipes from Instagram reels/posts by pasting the URL in the existing "Web" tab — the backend detects Instagram, extracts caption from meta tags, and sends it to AI for parsing.

**Architecture:** Enhance `parse_url()` in `backend/src/ai/ingest.rs` with Instagram detection (URL host check) and meta tag extraction. Move `reqwest::Client` to `AppState` with proper UA and timeout. Frontend passes `source_url` through the form and shows Instagram-specific toast.

**Tech Stack:** Rust/Axum, reqwest, scraper, Vue 3, vue-toastification

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `backend/src/lib.rs` | Modify | Add `http_client: reqwest::Client` to `AppState` |
| `backend/src/main.rs` | Modify | Build configured `reqwest::Client`, add to `AppState` |
| `backend/src/ai/ingest.rs` | Modify | Instagram detection, meta tag extraction, accept `&reqwest::Client` |
| `backend/src/routes/ingest.rs` | Modify | Pass `http_client` from `AppState` to `parse_url` |
| `frontend/src/pages/RecipeNewPage.vue` | Modify | Pass `source_url` to preview, Instagram toast |
| `frontend/src/components/RecipeForm.vue` | Modify | Include `source_url` in form data |

---

### Task 1: Move reqwest::Client to AppState with UA and timeout

**Files:**
- Modify: `backend/src/lib.rs:17-21`
- Modify: `backend/src/main.rs:29-32`
- Modify: `backend/src/routes/ingest.rs:90-93`
- Modify: `backend/src/ai/ingest.rs:123-157`

- [ ] **Step 1: Add `http_client` to `AppState`**

In `backend/src/lib.rs`, add the field:

```rust
#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub config: Arc<config::Config>,
    pub http_client: reqwest::Client,
}
```

- [ ] **Step 2: Build the client in `main.rs`**

In `backend/src/main.rs`, before creating `AppState`:

```rust
let http_client = reqwest::Client::builder()
    .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36")
    .timeout(std::time::Duration::from_secs(10))
    .build()?;

let state = AppState {
    pool,
    config: Arc::new(config),
    http_client,
};
```

- [ ] **Step 3: Update `parse_url` to accept `&reqwest::Client` instead of creating one**

In `backend/src/ai/ingest.rs`, change the signature:

```rust
pub async fn parse_url(
    client: &AnthropicClient,
    http_client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<ParsedRecipe> {
    let html = http_client.get(url).send().await?.text().await?;
```

Remove the `reqwest::Client::new()` that was previously inline.

- [ ] **Step 4: Update the route handler to pass `http_client`**

In `backend/src/routes/ingest.rs`, change the `"url"` arm:

```rust
"url" => {
    let url = url
        .filter(|u| !u.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("Zadejte URL receptu".into()))?;
    ai::ingest::parse_url(&ai_client, &state.http_client, &url)
        .await
        .map_err(AppError::Internal)?
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo check`
Expected: compiles without errors

---

### Task 2: Instagram URL detection and meta tag extraction

**Files:**
- Modify: `backend/src/ai/ingest.rs`

- [ ] **Step 1: Add `is_instagram_url` helper**

Add at the bottom of `backend/src/ai/ingest.rs`:

```rust
fn is_instagram_url(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.ends_with("instagram.com")))
        .unwrap_or(false)
}
```

- [ ] **Step 2: Add `extract_instagram_caption` function**

Add to `backend/src/ai/ingest.rs`:

```rust
/// Extract caption and author from Instagram HTML meta tags.
/// Returns (caption, author) or an error if extraction fails.
fn extract_instagram_caption(html: &str) -> anyhow::Result<(String, Option<String>)> {
    let document = scraper::Html::parse_document(html);

    // Extract <meta name="description" content="...">
    let desc_selector = scraper::Selector::parse(r#"meta[name="description"]"#).unwrap();
    let caption = document
        .select(&desc_selector)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Nepodařilo se načíst recept z Instagramu. \
                 Zkuste zkopírovat popisek z Instagramu a vložit ho jako text."
            )
        })?;

    // Validate it's not a generic Instagram page (login wall)
    if caption.len() < 20 || caption.starts_with("Instagram") {
        anyhow::bail!(
            "Nepodařilo se načíst recept z Instagramu. \
             Zkuste zkopírovat popisek z Instagramu a vložit ho jako text."
        );
    }

    // Extract author from <meta property="og:title" content="Author na Instagramu: ...">
    let og_title_selector = scraper::Selector::parse(r#"meta[property="og:title"]"#).unwrap();
    let author = document
        .select(&og_title_selector)
        .next()
        .and_then(|el| el.value().attr("content"))
        .and_then(|s| s.split(" na Instagramu").next())
        .map(|s| s.trim().to_string());

    Ok((caption, author))
}
```

- [ ] **Step 3: Add Instagram branch to `parse_url`**

Modify `parse_url` in `backend/src/ai/ingest.rs` to check for Instagram before the generic extraction:

```rust
pub async fn parse_url(
    client: &AnthropicClient,
    http_client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<ParsedRecipe> {
    let response = http_client.get(url).send().await?;

    // Check if we were redirected to a login page
    let final_url = response.url().to_string();
    if final_url.contains("/accounts/login") {
        anyhow::bail!(
            "Nepodařilo se načíst recept z Instagramu. \
             Zkuste zkopírovat popisek z Instagramu a vložit ho jako text."
        );
    }

    let html = response.text().await?;

    if is_instagram_url(url) {
        let (caption, author) = extract_instagram_caption(&html)?;

        let text = match author {
            Some(ref a) => format!(
                "Source: Instagram post by {a}\nURL: {url}\n\nCaption:\n{caption}"
            ),
            None => format!(
                "Source: Instagram post\nURL: {url}\n\nCaption:\n{caption}"
            ),
        };

        return parse_text(client, &text).await;
    }

    // Existing generic URL extraction
    let text = {
        let document = scraper::Html::parse_document(&html);
        let extracted = ["article", "main", "body"]
            .iter()
            .find_map(|tag| {
                let selector = scraper::Selector::parse(tag).ok()?;
                document
                    .select(&selector)
                    .next()
                    .map(|el| el.text().collect::<Vec<_>>().join(" "))
            })
            .unwrap_or_else(|| document.root_element().text().collect::<Vec<_>>().join(" "));

        if extracted.len() > 8000 {
            let mut end = 8000;
            while !extracted.is_char_boundary(end) {
                end -= 1;
            }
            extracted[..end].to_string()
        } else {
            extracted
        }
    };

    parse_text(client, &text).await
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo check`
Expected: compiles without errors

---

### Task 3: Frontend — source_url passthrough and Instagram toast

**Files:**
- Modify: `frontend/src/pages/RecipeNewPage.vue:86-115`
- Modify: `frontend/src/components/RecipeForm.vue:174-185`

- [ ] **Step 1: Pass `source_url` on preview result in RecipeNewPage**

In `frontend/src/pages/RecipeNewPage.vue`, in the `handleIngest()` function, after `result.source_type = activeTab.value` (line 105), add:

```typescript
result.source_type = activeTab.value
if (activeTab.value === 'url') {
  result.source_url = urlInput.value
}
```

- [ ] **Step 2: Add Instagram-specific toast message**

In the same `handleIngest()` function, change the toast logic (lines 88-92):

```typescript
const toastId = activeTab.value === 'photo'
  ? toast.info('Zpracovávám fotku...', { timeout: false })
  : activeTab.value === 'url'
    ? toast.info(
        urlInput.value.includes('instagram.com')
          ? 'Importuji z Instagramu...'
          : 'Stahuji recept...',
        { timeout: false }
      )
    : null
```

- [ ] **Step 3: Include `source_url` in RecipeForm data**

In `frontend/src/components/RecipeForm.vue`, add `source_url` to the `form` reactive object (line 174-185):

```typescript
const form = reactive({
  title: props.initial?.title || '',
  description: props.initial?.description || '',
  emoji: props.initial?.emoji || null,
  servings: props.initial?.servings || null,
  prep_time_min: props.initial?.prep_time_min || null,
  cook_time_min: props.initial?.cook_time_min || null,
  ingredients: props.initial?.ingredients || [],
  steps: props.initial?.steps || [],
  tags: props.initial?.tags || [],
  source_type: props.initial?.source_type || 'manual',
  source_url: props.initial?.source_url || null,
})
```

- [ ] **Step 4: Verify frontend builds**

Run: `cd /home/jenda.kolena/dev/vareni/frontend && npm run build`
Expected: builds without errors

---

### Task 4: End-to-end manual test

- [ ] **Step 1: Start the backend**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo run`

- [ ] **Step 2: Start the frontend dev server**

Run: `cd /home/jenda.kolena/dev/vareni/frontend && npm run dev`

- [ ] **Step 3: Test Instagram URL ingestion**

1. Open `http://localhost:5173` in browser
2. Go to "Nový recept" → "Web" tab
3. Paste: `https://www.instagram.com/reels/DCwqunhI7l6/`
4. Click "Zpracovat"
5. Verify:
   - Toast shows "Importuji z Instagramu..."
   - After processing, preview shows parsed recipe with "Hähnchen-Pfanne" / chicken content translated to Czech
   - Ingredients are populated (cibule, olej, kuřecí, paprika, etc.)
   - Steps may be marked as guessed (amber ring)
   - After saving, recipe detail page shows "instagram.com" as source link

- [ ] **Step 4: Test regular URL still works**

1. Go to "Nový recept" → "Web" tab
2. Paste a regular recipe URL (e.g. from kuchynelidlu.cz)
3. Verify it still works as before (body text extraction, not meta tag extraction)

- [ ] **Step 5: Test fallback error**

1. Paste an Instagram URL that doesn't exist: `https://www.instagram.com/reels/INVALID_SHORTCODE_999/`
2. Verify an error toast appears with the fallback message about pasting caption as text
