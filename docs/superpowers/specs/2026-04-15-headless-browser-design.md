# Headless Browser for SPA Recipe Scraping

## Problem

6 recipe sites cannot be scraped with the plain reqwest HTTP client:

| Site | Reason | Language |
|------|--------|----------|
| billa.cz | Vue.js SPA — no server-rendered recipe content | cs |
| albert.cz | Next.js SPA — search results JS-rendered | cs |
| receptynakazdyden.cz | TLS fingerprinting — reqwest gets 403 despite correct headers | cs |
| vareni.cz | Search results JS-rendered (category links only in raw HTML) | cs |
| bbcgoodfood.com | Search results JS-rendered (collection links only in raw HTML) | en |
| budgetbytes.com | Cloudflare JS challenge — reqwest gets 403 | en |

All 6 work fine in a real browser. A headless Chromium instance solves all three root causes (SPA rendering, TLS fingerprint, JS challenges) with a single mechanism.

## Approach

Use `chromiumoxide` (Rust-native Chrome DevTools Protocol client) to drive a headless Chromium process. The browser is launched on-demand per discovery run or per manual URL ingestion request, not kept alive as a long-lived process.

### Why chromiumoxide

- Async (tokio-native) — fits the existing Axum/tokio stack
- No Node.js sidecar needed — drives Chromium directly via CDP
- Chromium is required either way; this avoids an extra runtime

## Architecture

### 1. New module: `browser.rs`

Two public functions:

```rust
/// Launch a headless Chromium instance.
/// Returns the Browser handle and a spawned connection task handle.
pub async fn launch() -> anyhow::Result<(Browser, tokio::task::JoinHandle<()>)>

/// Fetch fully-rendered HTML from a URL using a browser tab.
/// Opens a new tab, navigates to the URL, waits for content, extracts HTML, closes the tab.
pub async fn fetch_html(
    browser: &Browser,
    url: &str,
    wait: WaitCondition,
    timeout: Duration,
) -> anyhow::Result<String>
```

`WaitCondition` enum:
- `Selector(&'static str)` — wait for a CSS selector to appear in DOM
- `NetworkIdle` — wait for no network activity for 500ms (avoid as default — modern sites with analytics/websockets may never reach idle, causing 30s timeouts)

The `launch()` function:
- Finds Chromium via `CHROME_PATH` env var, falling back to common paths (`/usr/bin/chromium`, `/usr/bin/chromium-browser`, `/usr/bin/google-chrome`)
- Launches with `--no-sandbox --disable-gpu --disable-dev-shm-usage --headless`
- Returns both the `Browser` and the connection handler `JoinHandle` (chromiumoxide requires the connection future to be polled)

The `fetch_html()` function:
- Opens a new tab (page)
- Navigates to the URL
- Waits per the `WaitCondition` or times out
- Executes `document.documentElement.outerHTML` via CDP to get fully-rendered HTML
- Closes the tab
- Returns the HTML string (same format as `reqwest::Response::text()`, so downstream parsing is unchanged)

### 2. RecipeProvider trait changes

Add two default methods:

```rust
/// Whether this provider requires a headless browser instead of plain HTTP.
fn requires_browser(&self) -> bool { false }

/// What to wait for after navigation before extracting HTML.
/// Only meaningful when `requires_browser()` is true.
/// Providers requiring a browser MUST override this with a specific Selector.
fn wait_condition(&self) -> WaitCondition { WaitCondition::NetworkIdle }
```

Existing providers are unaffected (defaults to `false`). All browser-requiring providers must override `wait_condition()` with a `Selector` — `NetworkIdle` is only a fallback and will timeout on most modern sites.

### 3. Changes to `fetch_recipe_urls`

Current signature:
```rust
pub async fn fetch_recipe_urls(
    client: &reqwest::Client,
    provider: &dyn RecipeProvider,
    prompt: Option<&str>,
    max_urls: usize,
) -> Result<Vec<String>, String>
```

New signature:
```rust
pub async fn fetch_recipe_urls(
    client: &reqwest::Client,
    browser: Option<&Browser>,
    provider: &dyn RecipeProvider,
    prompt: Option<&str>,
    max_urls: usize,
) -> Result<Vec<String>, String>
```

Logic change: if `provider.requires_browser()` and `browser` is `Some`, use `browser::fetch_html()` instead of `client.get()` to obtain the HTML. The rest (CSS selector extraction, URL filtering) is unchanged.

If the provider requires browser but no browser is available, return an error for that provider (logged as a warning, not fatal).

### 4. Changes to `parse_url` in `ai/ingest.rs`

Current signature:
```rust
pub async fn parse_url(
    client: &AnthropicClient,
    http_client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<ParsedRecipe>
```

New signature:
```rust
pub async fn parse_url(
    client: &AnthropicClient,
    http_client: &reqwest::Client,
    browser: Option<&Browser>,
    url: &str,
) -> anyhow::Result<ParsedRecipe>
```

The function checks whether the URL's domain belongs to a browser-required site via `needs_browser(url: &str) -> bool`. This function instantiates `providers()` and checks if any provider with `requires_browser() == true` matches the URL's domain. The providers are zero-sized structs so instantiation is free — no hardcoded domain list to maintain separately.

If browser is needed and available, use `browser::fetch_html()`. Otherwise fall back to reqwest (may fail for browser-required domains, but that's the current behavior anyway).

### 5. Changes to discovery route (`routes/discover.rs`)

At the start of `discover()`:
1. Check if any provider has `requires_browser() == true`
2. If yes, attempt to launch a browser via `browser::launch()`
3. If launch fails, log a warning and continue (reqwest-only providers still work)
4. Pass `Option<&Browser>` through to `fetch_recipe_urls` and `process_candidate` → `parse_url`
5. After the discovery loop, the `Browser` is dropped (Chromium process exits)

### 6. Changes to ingest route (`routes/ingest.rs`)

For the `"url"` ingestion path:
1. Check `needs_browser(url)` on the submitted URL
2. If yes, launch a browser for this single request
3. Pass to `parse_url`
4. Browser is dropped after the request

Concurrency guard: use a `tokio::sync::Semaphore` (permits=1) stored in `AppState` to ensure only one Chromium process runs at a time. Both the discovery route and the ingest route acquire a permit before launching a browser. If the permit is unavailable, the request waits (discovery is already slow; a few extra seconds is acceptable).

### 7. Provider implementations

#### Existing: `ReceptyNaKazdyDen` (modify)

Add override:
```rust
fn requires_browser(&self) -> bool { true }
fn wait_condition(&self) -> WaitCondition {
    WaitCondition::Selector("a[href*=\"/recept/\"]")
}
```

The existing `listing_url`, `link_selector`, `is_recipe_url` remain unchanged.

#### New: `BillaCz`

- base_url: `https://www.billa.cz`
- listing_url: `/recepty` (search: `/recepty/hledani?q={query}`)
- link_selector: `a[href*="/recepty/"]` (needs browser-rendered DOM to find recipe cards)
- is_recipe_url: URL matches `/recepty/{slug}` pattern, excluding `/recepty/hledani`, `/recepty/kategorie/`
- language: cs
- requires_browser: true
- wait_condition: `Selector("a[href*='/recepty/']")`

#### New: `AlbertCz`

- base_url: `https://www.albert.cz`
- listing_url: `/recepty` (search: `/recepty/vyhledavani?q={query}`)
- link_selector: `a[href*="/recept/"]`
- is_recipe_url: URL matches `/recept/{slug}` pattern
- language: cs
- requires_browser: true
- wait_condition: `Selector("a[href*='/recept/']")`

#### New: `VareniCz`

- base_url: `https://www.vareni.cz`
- listing_url: `/recepty/` (search: `/recepty/hledani/?q={query}`)
- link_selector: `a[href*="/recepty/"]`
- is_recipe_url: URL matches individual recipe pattern (e.g. `/recepty/{slug}/` but not category pages like `/recepty/kategorie/`)
- language: cs
- requires_browser: true
- wait_condition: `Selector("a[href*='/recepty/']")`

#### New: `BbcGoodFood`

- base_url: `https://www.bbcgoodfood.com`
- listing_url: `/recipes` (search: `/search?q={query}`)
- link_selector: `a[href*="/recipes/"]`
- is_recipe_url: URL matches `/recipes/{slug}` but not `/recipes/collection/`, `/recipes/category/`
- language: en
- requires_browser: true
- wait_condition: `Selector("a[href*='/recipes/']")`

#### New: `BudgetBytes`

- base_url: `https://www.budgetbytes.com`
- listing_url: `/category/recipes/` (search: `/?s={query}`)
- link_selector: `a[href*="budgetbytes.com/"]`
- is_recipe_url: URL matches individual post pattern (not `/category/`, not `/tag/`, has slug)
- language: en
- requires_browser: true
- wait_condition: `Selector("article a")`

### 8. Dockerfile changes

Add Chromium and required fonts to the Docker image:

```dockerfile
RUN apt-get update && apt-get install -y --no-install-recommends \
    chromium \
    fonts-liberation \
    && rm -rf /var/lib/apt/lists/*

ENV CHROME_PATH=/usr/bin/chromium
```

This adds ~300-400 MB to the image. Acceptable for a personal cooking app.

### 9. Error handling

- **Browser launch failure**: Log warning, set `browser = None`. All browser-requiring providers are skipped with an error entry in `errors[]`. Reqwest providers continue normally.
- **Page navigation timeout** (30s default): Skip that URL, increment `skipped.failed`, log warning. Continue with next URL.
- **Tab crash**: Catch the error from chromiumoxide, skip URL, continue. The browser process itself should survive individual tab crashes.
- **Chromium not found**: `launch()` returns an error. Same handling as launch failure.
- **Orphaned processes**: `chromiumoxide::Browser::drop()` sends a kill signal to the child process. As a safety net, `launch()` should verify this behavior. If the discovery handler panics or the HTTP connection drops, Rust's Drop guarantees still fire (unless the process itself is killed with SIGKILL). The `JoinHandle` for the CDP connection is aborted when the `Browser` is dropped.

### 10. Testing

- Unit tests for each new provider's `is_recipe_url()` and `listing_url()` — same pattern as existing provider tests.
- The `browser.rs` module is not unit-testable without Chromium installed. Integration testing is manual: run the app with Chromium available and trigger discovery.
- The `fetch_recipe_urls` function's reqwest path is already tested. The browser path follows the same code after HTML acquisition, so coverage comes from the existing tests + manual browser verification.

### 11. Config

New optional env var:
- `CHROME_PATH` — path to Chromium binary. Falls back to auto-detection if not set.

Add a `browser_semaphore: Arc<tokio::sync::Semaphore>` to `AppState` (permits=1) to prevent concurrent Chromium launches. No changes to `Config` struct — the browser module reads `CHROME_PATH` directly from the environment. The browser itself is ephemeral (not stored in AppState).
