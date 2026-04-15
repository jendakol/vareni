# Headless Browser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add headless Chromium support so the recipe scraper can handle SPA sites, TLS-fingerprinted sites, and Cloudflare JS challenges.

**Architecture:** A new `browser.rs` module wraps `chromiumoxide` to launch/kill Chromium and fetch rendered HTML. The `RecipeProvider` trait gains `requires_browser()` and `wait_condition()` methods. A tokio `Semaphore(1)` in `AppState` prevents concurrent browser launches. Six sites gain browser-backed providers.

**Tech Stack:** chromiumoxide (Rust CDP client), tokio, Chromium (installed in Docker image)

---

### Task 1: Add chromiumoxide dependency

**Files:**
- Modify: `backend/Cargo.toml`

- [ ] **Step 1: Add chromiumoxide to dependencies**

In `backend/Cargo.toml`, add to `[dependencies]`:

```toml
chromiumoxide = { version = "0.7", features = ["tokio-runtime"], default-features = false }
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo check`
Expected: compiles without errors

---

### Task 2: Create `browser.rs` module

**Files:**
- Create: `backend/src/browser.rs`
- Modify: `backend/src/lib.rs`

- [ ] **Step 1: Create the browser module with WaitCondition, launch, and fetch_html**

Create `backend/src/browser.rs`:

```rust
//! Headless Chromium browser for rendering SPA pages.
//!
//! Provides [`launch`] to start a browser and [`fetch_html`] to get fully-rendered
//! HTML from a page that requires JavaScript execution.

use std::time::Duration;

use chromiumoxide::Browser;
use chromiumoxide::BrowserConfig;

/// What to wait for after navigation before extracting HTML.
#[derive(Debug, Clone)]
pub enum WaitCondition {
    /// Wait for a CSS selector to appear in the DOM.
    Selector(&'static str),
    /// Wait for no network activity for 500ms.
    /// Warning: modern sites with analytics/websockets may never reach idle.
    NetworkIdle,
}

/// Launch a headless Chromium instance.
///
/// Returns the `Browser` handle and a `JoinHandle` for the CDP connection
/// (chromiumoxide requires the connection future to be polled).
///
/// The browser process is killed when the `Browser` is dropped.
pub async fn launch() -> anyhow::Result<(Browser, tokio::task::JoinHandle<()>)> {
    let chrome_path = find_chrome()?;
    tracing::info!(path = %chrome_path, "Launching headless Chromium");

    let (browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .chrome_executable(chrome_path)
            .no_sandbox()
            .arg("--disable-gpu")
            .arg("--disable-dev-shm-usage")
            .arg("--headless")
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build browser config: {e}"))?,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to launch Chromium: {e}"))?;

    // Spawn a task to drive the CDP connection
    let handle = tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });

    Ok((browser, handle))
}

/// Fetch fully-rendered HTML from a URL using a browser tab.
///
/// Opens a new tab, navigates to `url`, waits per `wait`, extracts
/// `document.documentElement.outerHTML`, and closes the tab.
pub async fn fetch_html(
    browser: &Browser,
    url: &str,
    wait: &WaitCondition,
    timeout: Duration,
) -> anyhow::Result<String> {
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| anyhow::anyhow!("Failed to open new tab: {e}"))?;

    // Navigate and wait with timeout
    let result = tokio::time::timeout(timeout, async {
        page.goto(url)
            .await
            .map_err(|e| anyhow::anyhow!("Navigation failed: {e}"))?;

        match wait {
            WaitCondition::Selector(sel) => {
                page.find_element(sel)
                    .await
                    .map_err(|e| anyhow::anyhow!("Wait for selector '{sel}' failed: {e}"))?;
            }
            WaitCondition::NetworkIdle => {
                // chromiumoxide waits for load event by default on goto;
                // add a small extra delay for late-loading content
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }

        // Extract rendered HTML
        let html: String = page
            .evaluate("document.documentElement.outerHTML")
            .await
            .map_err(|e| anyhow::anyhow!("Failed to extract HTML: {e}"))?
            .into_value()
            .map_err(|e| anyhow::anyhow!("Failed to deserialize HTML: {e}"))?;

        Ok::<String, anyhow::Error>(html)
    })
    .await;

    // Close the tab regardless of outcome
    let _ = page.close().await;

    match result {
        Ok(inner) => inner,
        Err(_) => anyhow::bail!("Page load timed out after {}s for {url}", timeout.as_secs()),
    }
}

/// Find the Chromium executable.
fn find_chrome() -> anyhow::Result<String> {
    if let Ok(path) = std::env::var("CHROME_PATH") {
        return Ok(path);
    }

    let candidates = [
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
    ];

    for path in candidates {
        if std::path::Path::new(path).exists() {
            return Ok(path.to_string());
        }
    }

    anyhow::bail!(
        "Chromium not found. Set CHROME_PATH or install chromium. Searched: {}",
        candidates.join(", ")
    )
}
```

- [ ] **Step 2: Register the module in lib.rs**

In `backend/src/lib.rs`, add after the `pub mod ai;` line:

```rust
pub mod browser;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo check`
Expected: compiles without errors (may have unused import warnings — that's fine for now)

---

### Task 3: Add `requires_browser` and `wait_condition` to RecipeProvider trait

**Files:**
- Modify: `backend/src/scraper.rs` (trait definition + import)

- [ ] **Step 1: Add the import for WaitCondition**

At the top of `backend/src/scraper.rs`, add after the existing `use scraper::{Html, Selector};` line:

```rust
use crate::browser::WaitCondition;
```

- [ ] **Step 2: Add the two new default methods to the trait**

In the `RecipeProvider` trait in `backend/src/scraper.rs`, add after the `fn is_recipe_url` method:

```rust
    /// Whether this provider requires a headless browser instead of plain HTTP.
    fn requires_browser(&self) -> bool {
        false
    }

    /// What to wait for after navigation before extracting HTML.
    /// Only meaningful when `requires_browser()` is true.
    /// Browser-requiring providers MUST override this with a Selector.
    fn wait_condition(&self) -> WaitCondition {
        WaitCondition::NetworkIdle
    }
```

- [ ] **Step 3: Add a `needs_browser` helper function**

After the `providers()` function in `backend/src/scraper.rs`, add:

```rust
/// Check if a URL belongs to a site that requires a headless browser.
///
/// Derives the answer from providers — no hardcoded domain list.
pub fn needs_browser(url: &str) -> bool {
    providers()
        .iter()
        .any(|p| p.requires_browser() && url.contains(p.name()))
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo check`
Expected: compiles without errors

---

### Task 4: Update `fetch_recipe_urls` to support browser

**Files:**
- Modify: `backend/src/scraper.rs` (the `fetch_recipe_urls` function)

- [ ] **Step 1: Add Browser import at the top of scraper.rs**

Add to the imports at the top of `backend/src/scraper.rs`:

```rust
use chromiumoxide::Browser;
```

- [ ] **Step 2: Update the function signature and HTML-fetching logic**

Replace the existing `fetch_recipe_urls` function in `backend/src/scraper.rs` with:

```rust
/// Fetch a listing/search page and extract recipe URLs.
pub async fn fetch_recipe_urls(
    client: &reqwest::Client,
    browser: Option<&Browser>,
    provider: &dyn RecipeProvider,
    prompt: Option<&str>,
    max_urls: usize,
) -> Result<Vec<String>, String> {
    let url = provider.listing_url(prompt);

    let html = if provider.requires_browser() {
        let browser = browser.ok_or_else(|| {
            format!(
                "{}: requires browser but none available",
                provider.name()
            )
        })?;
        crate::browser::fetch_html(
            browser,
            &url,
            &provider.wait_condition(),
            std::time::Duration::from_secs(30),
        )
        .await
        .map_err(|e| format!("{}: browser fetch failed: {e}", provider.name()))?
    } else {
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("{}: {e}", provider.name()))?;

        if !resp.status().is_success() {
            return Err(format!("{}: HTTP {}", provider.name(), resp.status()));
        }

        resp.text()
            .await
            .map_err(|e| format!("{}: failed to read body: {e}", provider.name()))?
    };

    let document = Html::parse_document(&html);
    let selector = Selector::parse(provider.link_selector())
        .map_err(|_| format!("{}: invalid CSS selector", provider.name()))?;

    let base_url = provider.base_url();
    let name = provider.name();

    let mut urls: Vec<String> = document
        .select(&selector)
        .filter_map(|el| el.value().attr("href"))
        .map(|href| {
            if href.starts_with("http") {
                href.to_string()
            } else {
                format!("{base_url}{href}")
            }
        })
        .filter(|u| {
            let valid = provider.is_recipe_url(u);
            if !valid {
                tracing::debug!(url = %u, site = name, "Filtered out non-recipe URL");
            }
            valid
        })
        .collect();

    let pre_filter_count = urls.len();

    // Deduplicate
    urls.sort();
    urls.dedup();

    tracing::info!(
        site = name,
        pre_filter = pre_filter_count,
        post_filter = urls.len(),
        "Scraped recipe URLs"
    );

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

- [ ] **Step 3: Verify it compiles**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo check`
Expected: compile error in `routes/discover.rs` because `fetch_recipe_urls` now expects a `browser` parameter — that's expected and will be fixed in Task 6.

---

### Task 5: Update `parse_url` to support browser

**Files:**
- Modify: `backend/src/ai/ingest.rs`

- [ ] **Step 1: Add Browser import**

At the top of `backend/src/ai/ingest.rs`, add:

```rust
use chromiumoxide::Browser;
```

- [ ] **Step 2: Update parse_url signature and HTML-fetching logic**

In `backend/src/ai/ingest.rs`, replace the `parse_url` function. Change only the signature and the HTML-fetching part at the top — the Instagram handling and everything after the `let html = ...` line stays the same:

```rust
pub async fn parse_url(
    client: &AnthropicClient,
    http_client: &reqwest::Client,
    browser: Option<&Browser>,
    url: &str,
) -> anyhow::Result<ParsedRecipe> {
    let (html, final_url) = if crate::scraper::needs_browser(url) {
        if let Some(browser) = browser {
            let html = crate::browser::fetch_html(
                browser,
                url,
                &crate::browser::WaitCondition::NetworkIdle,
                std::time::Duration::from_secs(30),
            )
            .await?;
            (html, url.to_string())
        } else {
            // No browser available — fall back to reqwest (may fail)
            let response = http_client.get(url).send().await?;
            let final_url = response.url().to_string();
            let html = response.text().await?;
            (html, final_url)
        }
    } else {
        let response = http_client.get(url).send().await?;
        let final_url = response.url().to_string();
        let html = response.text().await?;
        (html, final_url)
    };

    // Check final URL (after redirects) for Instagram — handles ig.me short links
    if is_instagram_url(url) || is_instagram_url(&final_url) {
```

The rest of the function body stays exactly the same, except change the reference from `html` to the already-bound `html` variable (it's already correct since the binding name didn't change).

- [ ] **Step 3: Verify it compiles**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo check`
Expected: compile error in `routes/ingest.rs` and `routes/discover.rs` — expected, fixed in next tasks.

---

### Task 6: Add browser_semaphore to AppState and update discover route

**Files:**
- Modify: `backend/src/lib.rs` (AppState)
- Modify: `backend/src/main.rs` (AppState construction)
- Modify: `backend/src/routes/discover.rs`

- [ ] **Step 1: Add semaphore to AppState**

In `backend/src/lib.rs`, add to the imports:

```rust
use tokio::sync::Semaphore;
```

Add a new field to the `AppState` struct:

```rust
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub config: Arc<config::Config>,
    pub http_client: reqwest::Client,
    pub embedding: Option<Arc<embedding::EmbeddingService>>,
    pub browser_semaphore: Arc<Semaphore>,
}
```

- [ ] **Step 2: Initialize the semaphore in main.rs**

In `backend/src/main.rs`, in the `AppState` construction block, add the new field:

```rust
    let state = AppState {
        pool,
        config: Arc::new(config),
        http_client,
        embedding,
        browser_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
    };
```

- [ ] **Step 3: Update the discover route to launch browser and pass it through**

In `backend/src/routes/discover.rs`, add at the top:

```rust
use chromiumoxide::Browser;
```

Then, in the `discover` function, after the translated_prompts section and before the `let mut all_urls` line, add browser launch logic:

```rust
    // Launch headless browser if any provider requires it
    let any_needs_browser = providers.iter().any(|p| p.requires_browser());
    let browser_guard;
    let browser_handle;
    let browser: Option<Browser> = if any_needs_browser {
        match state.browser_semaphore.clone().acquire_owned().await {
            Ok(permit) => {
                browser_guard = Some(permit);
                match crate::browser::launch().await {
                    Ok((b, h)) => {
                        browser_handle = Some(h);
                        Some(b)
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to launch browser — browser-requiring providers will be skipped");
                        browser_handle = None;
                        None
                    }
                }
            }
            Err(_) => {
                tracing::warn!("Browser semaphore closed");
                browser_guard = None;
                browser_handle = None;
                None
            }
        }
    } else {
        browser_guard = None;
        browser_handle = None;
        None
    };
    // Suppress unused variable warnings when browser isn't launched
    let _ = &browser_guard;
    let _ = &browser_handle;
```

- [ ] **Step 4: Update the fetch_recipe_urls call in discover**

In the `for provider in &providers` loop in `discover`, change the `scraper::fetch_recipe_urls` call from:

```rust
        match scraper::fetch_recipe_urls(
            &state.http_client,
            provider.as_ref(),
            prompt,
            urls_per_site,
        )
```

to:

```rust
        match scraper::fetch_recipe_urls(
            &state.http_client,
            browser.as_ref(),
            provider.as_ref(),
            prompt,
            urls_per_site,
        )
```

- [ ] **Step 5: Update the process_candidate call to pass browser**

In `routes/discover.rs`, update the `process_candidate` function signature to accept a browser:

```rust
#[allow(clippy::too_many_arguments)]
async fn process_candidate(
    state: &AppState,
    embedding_svc: &Arc<EmbeddingService>,
    client: &AnthropicClient,
    browser: Option<&Browser>,
    owner_id: uuid::Uuid,
    url: &str,
    user_prompt: Option<&str>,
    restrictions_json: &str,
    preferences_json: &str,
    existing_titles: &str,
    rejected_titles: &str,
) -> anyhow::Result<CandidateResult> {
```

And change the `parse_url` call inside `process_candidate` from:

```rust
    let parsed = crate::ai::ingest::parse_url(client, &state.http_client, url).await?;
```

to:

```rust
    let parsed = crate::ai::ingest::parse_url(client, &state.http_client, browser, url).await?;
```

Also update the call site in the `discover` function where `process_candidate` is called — add `browser.as_ref()` as the new fourth argument:

```rust
        let result = process_candidate(
            &state,
            embedding_svc,
            &client,
            browser.as_ref(),
            auth.user_id,
            url,
            body.prompt.as_deref(),
            &restrictions_json,
            &preferences_json,
            &existing_titles_str,
            &rejected_titles_str,
        )
        .await;
```

- [ ] **Step 6: Verify it compiles**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo check`
Expected: compile error in `routes/ingest.rs` — fixed in next task.

---

### Task 7: Update ingest route to support browser

**Files:**
- Modify: `backend/src/routes/ingest.rs`

- [ ] **Step 1: Update the "url" arm to launch browser if needed**

In `backend/src/routes/ingest.rs`, replace the `"url"` match arm:

```rust
        "url" => {
            let url = url
                .filter(|u| !u.trim().is_empty())
                .ok_or_else(|| AppError::BadRequest("Zadejte URL receptu".into()))?;

            let browser = if crate::scraper::needs_browser(&url) {
                let _permit = state.browser_semaphore.acquire().await
                    .map_err(|_| AppError::ServiceUnavailable("Browser unavailable".into()))?;
                match crate::browser::launch().await {
                    Ok((b, _handle)) => Some(b),
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to launch browser for URL ingest");
                        None
                    }
                }
            } else {
                None
            };

            ai::ingest::parse_url(&ai_client, &state.http_client, browser.as_ref(), &url)
                .await
                .map_err(|e| {
                    let msg = e.to_string();
                    if msg.starts_with("Nepodařilo") {
                        AppError::BadRequest(msg)
                    } else {
                        AppError::Internal(e)
                    }
                })?
        }
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo check`
Expected: compiles without errors. All call sites now pass the browser parameter.

- [ ] **Step 3: Run existing tests**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo test`
Expected: all existing tests pass (no behavioral change for non-browser providers)

---

### Task 8: Modify ReceptyNaKazdyDen provider to require browser

**Files:**
- Modify: `backend/src/scraper.rs` (ReceptyNaKazdyDen impl)

- [ ] **Step 1: Write tests for the new trait methods**

Add to the test module in `backend/src/scraper.rs`:

```rust
    // --- requires_browser ---

    #[test]
    fn rnakazdyden_requires_browser() {
        assert!(ReceptyNaKazdyDen.requires_browser());
    }

    #[test]
    fn reqwest_providers_dont_require_browser() {
        assert!(!FreshIprima.requires_browser());
        assert!(!KuchyneLidlu.requires_browser());
        assert!(!TopRecepty.requires_browser());
        assert!(!ApetitOnline.requires_browser());
        assert!(!ReceptyCz.requires_browser());
        assert!(!KauflandCz.requires_browser());
        assert!(!Chefkoch.requires_browser());
        assert!(!FoodNetworkUk.requires_browser());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo test rnakazdyden_requires_browser`
Expected: FAIL — `ReceptyNaKazdyDen.requires_browser()` returns `false`

- [ ] **Step 3: Add requires_browser and wait_condition to ReceptyNaKazdyDen**

In `backend/src/scraper.rs`, add to the `impl RecipeProvider for ReceptyNaKazdyDen` block:

```rust
    fn requires_browser(&self) -> bool {
        true
    }

    fn wait_condition(&self) -> WaitCondition {
        WaitCondition::Selector("a[href*=\"/recept/\"]")
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo test`
Expected: all tests pass

---

### Task 9: Add BillaCz provider

**Files:**
- Modify: `backend/src/scraper.rs`

- [ ] **Step 1: Write tests**

Add to the test module in `backend/src/scraper.rs`:

```rust
    // --- is_recipe_url: billa.cz ---

    #[test]
    fn billa_accepts_recipe() {
        let p = BillaCz;
        assert!(p.is_recipe_url("https://www.billa.cz/recepty/pikantni-kureci-stripsy"));
    }

    #[test]
    fn billa_accepts_recipe_with_trailing_slash() {
        let p = BillaCz;
        assert!(p.is_recipe_url("https://www.billa.cz/recepty/bramborovy-gulas/"));
    }

    #[test]
    fn billa_rejects_listing() {
        let p = BillaCz;
        assert!(!p.is_recipe_url("https://www.billa.cz/recepty"));
    }

    #[test]
    fn billa_rejects_search() {
        let p = BillaCz;
        assert!(!p.is_recipe_url("https://www.billa.cz/recepty/hledani?q=kure"));
    }

    #[test]
    fn billa_rejects_kategorie() {
        let p = BillaCz;
        assert!(!p.is_recipe_url("https://www.billa.cz/recepty/kategorie/hlavni-jidla"));
    }

    #[test]
    fn billa_requires_browser() {
        assert!(BillaCz.requires_browser());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo test billa`
Expected: FAIL — `BillaCz` not found

- [ ] **Step 3: Implement BillaCz provider**

Add to `backend/src/scraper.rs`, before the `providers()` function:

```rust
/// Provider for billa.cz (Vue.js SPA, requires browser).
pub struct BillaCz;

impl RecipeProvider for BillaCz {
    fn name(&self) -> &'static str {
        "billa.cz"
    }

    fn base_url(&self) -> &'static str {
        "https://www.billa.cz"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            Some(query) => {
                let keywords = simplify_query(query);
                format!(
                    "{}/recepty/hledani?q={}",
                    self.base_url(),
                    urlencoding::encode(&keywords)
                )
            }
            None => format!("{}/recepty", self.base_url()),
        }
    }

    fn link_selector(&self) -> &'static str {
        "a[href*=\"/recepty/\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        let path = url
            .strip_prefix("https://www.billa.cz/recepty/")
            .unwrap_or("");
        let slug = path.trim_end_matches('/');
        !slug.is_empty()
            && !slug.contains('/')
            && slug != "hledani"
            && !slug.starts_with("hledani?")
            && !slug.starts_with("kategorie")
    }

    fn requires_browser(&self) -> bool {
        true
    }

    fn wait_condition(&self) -> WaitCondition {
        WaitCondition::Selector("a[href*='/recepty/']")
    }
}
```

- [ ] **Step 4: Add BillaCz to the providers() list**

In the `providers()` function, add `Box::new(BillaCz),` after the ReceptyNaKazdyDen entry.

- [ ] **Step 5: Run tests**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo test billa`
Expected: all billa tests pass

---

### Task 10: Add AlbertCz provider

**Files:**
- Modify: `backend/src/scraper.rs`

- [ ] **Step 1: Write tests**

Add to the test module in `backend/src/scraper.rs`:

```rust
    // --- is_recipe_url: albert.cz ---

    #[test]
    fn albert_accepts_recipe() {
        let p = AlbertCz;
        assert!(p.is_recipe_url("https://www.albert.cz/recept/jednoduche-a-rychle-recepty-na-oblibene-testoviny"));
    }

    #[test]
    fn albert_accepts_recipe_with_trailing_slash() {
        let p = AlbertCz;
        assert!(p.is_recipe_url("https://www.albert.cz/recept/kureci-nudlicky-se-zeleninou/"));
    }

    #[test]
    fn albert_rejects_listing() {
        let p = AlbertCz;
        assert!(!p.is_recipe_url("https://www.albert.cz/recepty"));
    }

    #[test]
    fn albert_rejects_search() {
        let p = AlbertCz;
        assert!(!p.is_recipe_url("https://www.albert.cz/recepty/vyhledavani?q=kure"));
    }

    #[test]
    fn albert_requires_browser() {
        assert!(AlbertCz.requires_browser());
    }
```

- [ ] **Step 2: Implement AlbertCz provider**

Add to `backend/src/scraper.rs`, before the `providers()` function:

```rust
/// Provider for albert.cz (Next.js SPA, requires browser).
pub struct AlbertCz;

impl RecipeProvider for AlbertCz {
    fn name(&self) -> &'static str {
        "albert.cz"
    }

    fn base_url(&self) -> &'static str {
        "https://www.albert.cz"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            Some(query) => {
                let keywords = simplify_query(query);
                format!(
                    "{}/recepty/vyhledavani?q={}",
                    self.base_url(),
                    urlencoding::encode(&keywords)
                )
            }
            None => format!("{}/recepty", self.base_url()),
        }
    }

    fn link_selector(&self) -> &'static str {
        "a[href*=\"/recept/\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        let path = url
            .strip_prefix("https://www.albert.cz/recept/")
            .unwrap_or("");
        let slug = path.trim_end_matches('/');
        !slug.is_empty() && !slug.contains('/')
    }

    fn requires_browser(&self) -> bool {
        true
    }

    fn wait_condition(&self) -> WaitCondition {
        WaitCondition::Selector("a[href*='/recept/']")
    }
}
```

- [ ] **Step 3: Add AlbertCz to the providers() list**

Add `Box::new(AlbertCz),` to the `providers()` function.

- [ ] **Step 4: Run tests**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo test albert`
Expected: all albert tests pass

---

### Task 11: Add VareniCz provider

**Files:**
- Modify: `backend/src/scraper.rs`

- [ ] **Step 1: Write tests**

Add to the test module in `backend/src/scraper.rs`:

```rust
    // --- is_recipe_url: vareni.cz ---

    #[test]
    fn vareni_accepts_recipe() {
        let p = VareniCz;
        assert!(p.is_recipe_url("https://www.vareni.cz/recepty/kureci-stehna-na-kari/"));
    }

    #[test]
    fn vareni_accepts_recipe_without_trailing_slash() {
        let p = VareniCz;
        assert!(p.is_recipe_url("https://www.vareni.cz/recepty/bramborovy-gulas"));
    }

    #[test]
    fn vareni_rejects_listing() {
        let p = VareniCz;
        assert!(!p.is_recipe_url("https://www.vareni.cz/recepty/"));
    }

    #[test]
    fn vareni_rejects_kategorie() {
        let p = VareniCz;
        assert!(!p.is_recipe_url("https://www.vareni.cz/recepty/kategorie/jidla-bez-masa/"));
    }

    #[test]
    fn vareni_rejects_hledani() {
        let p = VareniCz;
        assert!(!p.is_recipe_url("https://www.vareni.cz/recepty/hledani/?q=kure"));
    }

    #[test]
    fn vareni_rejects_fotorecepty() {
        let p = VareniCz;
        assert!(!p.is_recipe_url("https://www.vareni.cz/fotorecepty/"));
    }

    #[test]
    fn vareni_requires_browser() {
        assert!(VareniCz.requires_browser());
    }
```

- [ ] **Step 2: Implement VareniCz provider**

Add to `backend/src/scraper.rs`, before the `providers()` function:

```rust
/// Provider for vareni.cz (search results JS-rendered, requires browser).
pub struct VareniCz;

impl RecipeProvider for VareniCz {
    fn name(&self) -> &'static str {
        "vareni.cz"
    }

    fn base_url(&self) -> &'static str {
        "https://www.vareni.cz"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            Some(query) => {
                let keywords = simplify_query(query);
                format!(
                    "{}/recepty/hledani/?q={}",
                    self.base_url(),
                    urlencoding::encode(&keywords)
                )
            }
            None => format!("{}/recepty/", self.base_url()),
        }
    }

    fn link_selector(&self) -> &'static str {
        "a[href*=\"/recepty/\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        let path = url
            .strip_prefix("https://www.vareni.cz/recepty/")
            .unwrap_or("");
        let slug = path.trim_end_matches('/');
        // Must be a single-segment slug
        !slug.is_empty()
            && !slug.contains('/')
            && !slug.starts_with("kategorie")
            && !slug.starts_with("hledani")
    }

    fn requires_browser(&self) -> bool {
        true
    }

    fn wait_condition(&self) -> WaitCondition {
        WaitCondition::Selector("a[href*='/recepty/']")
    }
}
```

- [ ] **Step 3: Add VareniCz to the providers() list**

Add `Box::new(VareniCz),` to the `providers()` function.

- [ ] **Step 4: Run tests**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo test vareni`
Expected: all vareni tests pass

---

### Task 12: Add BbcGoodFood provider

**Files:**
- Modify: `backend/src/scraper.rs`

- [ ] **Step 1: Write tests**

Add to the test module in `backend/src/scraper.rs`:

```rust
    // --- is_recipe_url: bbcgoodfood.com ---

    #[test]
    fn bbcgoodfood_accepts_recipe() {
        let p = BbcGoodFood;
        assert!(p.is_recipe_url("https://www.bbcgoodfood.com/recipes/chicken-tikka-masala"));
    }

    #[test]
    fn bbcgoodfood_accepts_recipe_with_trailing_slash() {
        let p = BbcGoodFood;
        assert!(p.is_recipe_url("https://www.bbcgoodfood.com/recipes/easy-pasta-salad/"));
    }

    #[test]
    fn bbcgoodfood_rejects_collection() {
        let p = BbcGoodFood;
        assert!(!p.is_recipe_url("https://www.bbcgoodfood.com/recipes/collection/pasta-recipes"));
    }

    #[test]
    fn bbcgoodfood_rejects_category() {
        let p = BbcGoodFood;
        assert!(!p.is_recipe_url("https://www.bbcgoodfood.com/recipes/category/special-occasion-collections"));
    }

    #[test]
    fn bbcgoodfood_rejects_listing() {
        let p = BbcGoodFood;
        assert!(!p.is_recipe_url("https://www.bbcgoodfood.com/recipes"));
    }

    #[test]
    fn bbcgoodfood_requires_browser() {
        assert!(BbcGoodFood.requires_browser());
    }

    #[test]
    fn bbcgoodfood_language_is_english() {
        assert_eq!(BbcGoodFood.language(), "en");
    }
```

- [ ] **Step 2: Implement BbcGoodFood provider**

Add to `backend/src/scraper.rs`, before the `providers()` function:

```rust
/// Provider for bbcgoodfood.com (search results JS-rendered, requires browser).
pub struct BbcGoodFood;

impl RecipeProvider for BbcGoodFood {
    fn name(&self) -> &'static str {
        "bbcgoodfood.com"
    }

    fn base_url(&self) -> &'static str {
        "https://www.bbcgoodfood.com"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            Some(query) => {
                format!(
                    "{}/search?q={}",
                    self.base_url(),
                    urlencoding::encode(query)
                )
            }
            None => format!("{}/recipes", self.base_url()),
        }
    }

    fn link_selector(&self) -> &'static str {
        "a[href*=\"/recipes/\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        let path = url
            .strip_prefix("https://www.bbcgoodfood.com/recipes/")
            .unwrap_or("");
        let slug = path.trim_end_matches('/');
        !slug.is_empty()
            && !slug.contains('/')
            && !slug.starts_with("collection")
            && !slug.starts_with("category")
    }

    fn language(&self) -> &'static str {
        "en"
    }

    fn requires_browser(&self) -> bool {
        true
    }

    fn wait_condition(&self) -> WaitCondition {
        WaitCondition::Selector("a[href*='/recipes/']")
    }
}
```

- [ ] **Step 3: Add BbcGoodFood to the providers() list**

Add `Box::new(BbcGoodFood),` to the `providers()` function.

- [ ] **Step 4: Run tests**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo test bbcgoodfood`
Expected: all bbcgoodfood tests pass

---

### Task 13: Add BudgetBytes provider

**Files:**
- Modify: `backend/src/scraper.rs`

- [ ] **Step 1: Write tests**

Add to the test module in `backend/src/scraper.rs`:

```rust
    // --- is_recipe_url: budgetbytes.com ---

    #[test]
    fn budgetbytes_accepts_recipe() {
        let p = BudgetBytes;
        assert!(p.is_recipe_url("https://www.budgetbytes.com/creamy-garlic-chicken-pasta/"));
    }

    #[test]
    fn budgetbytes_accepts_recipe_without_trailing_slash() {
        let p = BudgetBytes;
        assert!(p.is_recipe_url("https://www.budgetbytes.com/one-pot-chili-pasta"));
    }

    #[test]
    fn budgetbytes_rejects_category() {
        let p = BudgetBytes;
        assert!(!p.is_recipe_url("https://www.budgetbytes.com/category/recipes/"));
    }

    #[test]
    fn budgetbytes_rejects_tag() {
        let p = BudgetBytes;
        assert!(!p.is_recipe_url("https://www.budgetbytes.com/tag/chicken/"));
    }

    #[test]
    fn budgetbytes_rejects_about() {
        let p = BudgetBytes;
        assert!(!p.is_recipe_url("https://www.budgetbytes.com/about/"));
    }

    #[test]
    fn budgetbytes_rejects_nested_path() {
        let p = BudgetBytes;
        assert!(!p.is_recipe_url("https://www.budgetbytes.com/category/recipes/chicken/"));
    }

    #[test]
    fn budgetbytes_requires_browser() {
        assert!(BudgetBytes.requires_browser());
    }

    #[test]
    fn budgetbytes_language_is_english() {
        assert_eq!(BudgetBytes.language(), "en");
    }
```

- [ ] **Step 2: Implement BudgetBytes provider**

Add to `backend/src/scraper.rs`, before the `providers()` function:

```rust
/// Provider for budgetbytes.com (Cloudflare JS challenge, requires browser).
pub struct BudgetBytes;

impl RecipeProvider for BudgetBytes {
    fn name(&self) -> &'static str {
        "budgetbytes.com"
    }

    fn base_url(&self) -> &'static str {
        "https://www.budgetbytes.com"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            Some(query) => {
                format!(
                    "{}/?s={}",
                    self.base_url(),
                    urlencoding::encode(query)
                )
            }
            None => format!("{}/category/recipes/", self.base_url()),
        }
    }

    fn link_selector(&self) -> &'static str {
        "a[href*=\"budgetbytes.com/\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        let path = url
            .strip_prefix("https://www.budgetbytes.com/")
            .unwrap_or("");
        let slug = path.trim_end_matches('/');
        // Must be a single-segment slug (individual post)
        // Exclude known non-recipe paths
        !slug.is_empty()
            && !slug.contains('/')
            && !slug.starts_with("category")
            && !slug.starts_with("tag")
            && !slug.starts_with("about")
            && !slug.starts_with("contact")
            && !slug.starts_with("privacy")
            && slug != "recipes"
    }

    fn language(&self) -> &'static str {
        "en"
    }

    fn requires_browser(&self) -> bool {
        true
    }

    fn wait_condition(&self) -> WaitCondition {
        WaitCondition::Selector("article a")
    }
}
```

- [ ] **Step 3: Add BudgetBytes to the providers() list**

Add `Box::new(BudgetBytes),` to the `providers()` function.

- [ ] **Step 4: Run all tests**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo test`
Expected: all tests pass

---

### Task 14: Update Dockerfile

**Files:**
- Modify: `Dockerfile`

- [ ] **Step 1: Add Chromium to the runtime stage**

In the `Dockerfile`, in the runtime stage (Stage 3), change the `apt-get` line from:

```dockerfile
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
```

to:

```dockerfile
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    chromium \
    fonts-liberation \
    && rm -rf /var/lib/apt/lists/*

ENV CHROME_PATH=/usr/bin/chromium
```

---

### Task 15: Run clippy and fmt

**Files:**
- All modified files

- [ ] **Step 1: Run clippy**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo clippy --all --tests`
Expected: no errors. Fix any warnings.

- [ ] **Step 2: Run fmt**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo fmt`

- [ ] **Step 3: Run all tests one final time**

Run: `cd /home/jenda.kolena/dev/vareni/backend && cargo test`
Expected: all tests pass

---

### Task 16: Manual integration test

- [ ] **Step 1: Verify Chromium is available**

Run: `which chromium || which chromium-browser || which google-chrome`
Expected: a path to a Chromium executable

- [ ] **Step 2: Start the backend**

```bash
screen -dmS vareni-backend bash -c \
  'RUST_LOG=cooking_app=debug EMBEDDING_MODEL_DIR=/home/jenda.kolena/dev/vareni/models/all-MiniLM-L6-v2 \
  cargo run 2>&1 | tee /tmp/claude/vareni-backend.log'
```

Wait for "Listening on 0.0.0.0:8080".

- [ ] **Step 3: Get auth token**

```bash
TOKEN=$(curl -s -X POST http://localhost:8080/api/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"name":"jenda","password":"1^BqTOKmI9Eb^@B00!*3"}' \
  | jq -r '.token')
```

- [ ] **Step 4: Run discovery and verify browser providers work**

```bash
curl -s -X POST http://localhost:8080/api/discover \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"kuřecí","count":3}' | jq '.discovered | length, .skipped, .errors'
```

Expected: some discovered recipes, and no errors for billa.cz, albert.cz, vareni.cz, receptynakazdyden.cz, bbcgoodfood.com, budgetbytes.com. Check the backend log for "Launching headless Chromium" messages.
