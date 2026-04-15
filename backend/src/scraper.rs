//! Site scraping: fetch recipe listing pages and extract recipe URLs.
//!
//! Each supported recipe site is represented by a [`RecipeProvider`] implementation
//! that encapsulates site-specific URL validation, search URL construction, and
//! CSS selectors.

use scraper::{Html, Selector};

use crate::browser::WaitCondition;

/// A recipe provider that can list and validate recipe URLs from a specific site.
pub trait RecipeProvider: Send + Sync {
    /// Human-readable site name (e.g. "fresh.iprima.cz").
    fn name(&self) -> &'static str;

    /// Base URL with scheme, no trailing slash (e.g. "https://fresh.iprima.cz").
    fn base_url(&self) -> &'static str;

    /// CSS selector for extracting links from the listing page.
    fn link_selector(&self) -> &'static str;

    /// Language code for this provider (e.g. "cs", "de", "en").
    /// Used to translate the user's Czech search query before constructing the search URL.
    fn language(&self) -> &'static str {
        "cs"
    }

    /// Build the full URL to fetch for listing or search.
    ///
    /// When `prompt` is `Some`, the provider should return a search URL if supported,
    /// falling back to the listing URL otherwise.
    fn listing_url(&self, prompt: Option<&str>) -> String;

    /// Is this URL a valid individual recipe page (not a listing, category, author, etc.)?
    fn is_recipe_url(&self, url: &str) -> bool;

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
}

/// Provider for fresh.iprima.cz.
pub struct FreshIprima;

impl RecipeProvider for FreshIprima {
    fn name(&self) -> &'static str {
        "fresh.iprima.cz"
    }

    fn base_url(&self) -> &'static str {
        "https://fresh.iprima.cz"
    }

    fn listing_url(&self, _prompt: Option<&str>) -> String {
        // No search support
        format!("{}/recepty", self.base_url())
    }

    fn link_selector(&self) -> &'static str {
        "a[href*=\"fresh.iprima.cz/\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        if !passes_common_filters(url, self.base_url()) {
            return false;
        }

        // Individual recipes end with a 6-digit numeric ID
        // Listicles start with numbers like "20-receptu-na..."
        // Category pages: /recepty/maso, /kuchari/*, /specialy/*
        let path = url.strip_prefix(self.base_url()).unwrap_or(url);
        let path = path.trim_start_matches('/');

        // Must end with digits (recipe ID)
        path.split('-').next_back().is_some_and(|last| {
            last.len() >= 5 && last.chars().all(|c| c.is_ascii_digit())
        })
        // Exclude subcategory pages
        && !path.starts_with("recepty/")
        && !path.starts_with("kuchari/")
        && !path.starts_with("specialy/")
    }
}

/// Provider for kuchynelidlu.cz.
pub struct KuchyneLidlu;

impl RecipeProvider for KuchyneLidlu {
    fn name(&self) -> &'static str {
        "kuchynelidlu.cz"
    }

    fn base_url(&self) -> &'static str {
        "https://kuchynelidlu.cz"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            Some(query) => {
                let keywords = simplify_query(query);
                format!(
                    "{}/recepty?search={}",
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
        if !passes_common_filters(url, self.base_url()) {
            return false;
        }
        // The link_selector already filters to /recept/ paths
        // Exclude: /recept/jak-pouzivat-* (how-to articles, not recipes)
        !url.contains("/jak-pouzivat-")
    }
}

/// Provider for receptyodanicky.cz.
pub struct ReceptyOdAnicky;

impl RecipeProvider for ReceptyOdAnicky {
    fn name(&self) -> &'static str {
        "receptyodanicky.cz"
    }

    fn base_url(&self) -> &'static str {
        "https://www.receptyodanicky.cz"
    }

    fn listing_url(&self, _prompt: Option<&str>) -> String {
        // No search support
        format!("{}/", self.base_url())
    }

    fn link_selector(&self) -> &'static str {
        "a[href*=\"www.receptyodanicky.cz/\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        if !passes_common_filters(url, self.base_url()) {
            return false;
        }

        // Recipe URLs: www.receptyodanicky.cz/recipe-slug/
        // Exclude: /author/, /category/, /o-mne/, /vsechny-recepty/, /spoluprace/, /newsletter/
        // Exclude: shop.receptyodanicky.cz (different subdomain)
        let path = url
            .strip_prefix("https://www.receptyodanicky.cz/")
            .unwrap_or("");

        !path.is_empty()
            && !path.starts_with("author/")
            && !path.starts_with("category/")
            && !path.starts_with("o-mne")
            && !path.starts_with("vsechny-recepty")
            && !path.starts_with("spoluprace")
            && !path.starts_with("newsletter")
            && !path.starts_with("recepty-dle-")
            && !path.starts_with("wp-")
            && !url.contains("shop.receptyodanicky.cz")
            // Should be a single path segment (recipe-slug/)
            && path.trim_end_matches('/').matches('/').count() == 0
    }
}

/// Provider for toprecepty.cz.
pub struct TopRecepty;

impl RecipeProvider for TopRecepty {
    fn name(&self) -> &'static str {
        "toprecepty.cz"
    }

    fn base_url(&self) -> &'static str {
        "https://www.toprecepty.cz"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            Some(query) => {
                let keywords = simplify_query(query);
                format!(
                    "{}/vyhledavani-receptu?term={}",
                    self.base_url(),
                    urlencoding::encode(&keywords)
                )
            }
            None => format!("{}/recepty/", self.base_url()),
        }
    }

    fn link_selector(&self) -> &'static str {
        "a[href*=\"/recept/\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        if !passes_common_filters(url, self.base_url()) {
            return false;
        }
        // Recipe URLs: toprecepty.cz/recept/12345-slug/
        // Must have a numeric ID after /recept/
        let path = url
            .strip_prefix("https://www.toprecepty.cz/recept/")
            .unwrap_or("");
        path.chars().next().is_some_and(|c| c.is_ascii_digit())
    }
}

/// Provider for apetitonline.cz.
pub struct ApetitOnline;

impl RecipeProvider for ApetitOnline {
    fn name(&self) -> &'static str {
        "apetitonline.cz"
    }

    fn base_url(&self) -> &'static str {
        "https://www.apetitonline.cz"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            Some(query) => {
                let keywords = simplify_query(query);
                format!(
                    "{}/recepty?q={}",
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
        if !passes_common_filters(url, self.base_url()) {
            return false;
        }
        // Recipe URLs: apetitonline.cz/recept/recipe-slug
        let path = url
            .strip_prefix("https://www.apetitonline.cz/recept/")
            .unwrap_or("");
        !path.is_empty() && !path.contains('?')
    }
}

/// Provider for recepty.cz.
pub struct ReceptyCz;

impl RecipeProvider for ReceptyCz {
    fn name(&self) -> &'static str {
        "recepty.cz"
    }

    fn base_url(&self) -> &'static str {
        "https://www.recepty.cz"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            Some(query) => {
                let keywords = simplify_query(query);
                format!(
                    "{}/vyhledavani?text={}",
                    self.base_url(),
                    urlencoding::encode(&keywords)
                )
            }
            None => format!("{}/", self.base_url()),
        }
    }

    fn link_selector(&self) -> &'static str {
        "a[href*=\"/recept/\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        if !passes_common_filters(url, self.base_url()) {
            return false;
        }
        // Recipe URLs: recepty.cz/recept/{slug}-{numeric_id}
        let path = url
            .strip_prefix("https://www.recepty.cz/recept/")
            .unwrap_or("");
        // Must end with -{digits} (recipe ID)
        path.rsplit_once('-')
            .is_some_and(|(_, id)| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()))
    }
}

/// Provider for kaufland.cz (prodejny.kaufland.cz).
pub struct KauflandCz;

impl RecipeProvider for KauflandCz {
    fn name(&self) -> &'static str {
        "kaufland.cz"
    }

    fn base_url(&self) -> &'static str {
        "https://prodejny.kaufland.cz"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            Some(query) => {
                let keywords = simplify_query(query);
                format!(
                    "{}/recepty/vyhledat-recept.html?searchsubmit=true&searchterm={}&recipes-search-category=all&time=all&difficulty=all",
                    self.base_url(),
                    urlencoding::encode(&keywords)
                )
            }
            None => format!("{}/recepty/hlavni-jidla.html", self.base_url()),
        }
    }

    fn link_selector(&self) -> &'static str {
        "a[href*=\"/recepty/vyhledat-recept/recept.\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        // Recipe URLs: /recepty/vyhledat-recept/recept.{slug}.r_id={id}.html
        url.contains("/recepty/vyhledat-recept/recept.")
            && url.contains("r_id=")
            && url.ends_with(".html")
    }
}

/// Provider for receptynakazdyden.cz.
pub struct ReceptyNaKazdyDen;

impl RecipeProvider for ReceptyNaKazdyDen {
    fn name(&self) -> &'static str {
        "receptynakazdyden.cz"
    }

    fn base_url(&self) -> &'static str {
        "https://www.receptynakazdyden.cz"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            Some(query) => {
                let keywords = simplify_query(query);
                format!("{}/?s={}", self.base_url(), urlencoding::encode(&keywords))
            }
            None => format!("{}/", self.base_url()),
        }
    }

    fn link_selector(&self) -> &'static str {
        "a[href*=\"/recept/\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        if !passes_common_filters(url, self.base_url()) {
            return false;
        }
        // Recipe URLs: receptynakazdyden.cz/recept/{slug}/
        let path = url
            .strip_prefix("https://www.receptynakazdyden.cz/recept/")
            .unwrap_or("");
        let slug = path.trim_end_matches('/');
        // Must be a non-empty slug, single segment (no nested paths)
        !slug.is_empty() && !slug.contains('/')
    }

    fn requires_browser(&self) -> bool {
        true
    }

    fn wait_condition(&self) -> WaitCondition {
        WaitCondition::Selector("a[href*=\"/recept/\"]")
    }
}

/// Provider for chefkoch.de (German).
pub struct Chefkoch;

impl RecipeProvider for Chefkoch {
    fn name(&self) -> &'static str {
        "chefkoch.de"
    }

    fn base_url(&self) -> &'static str {
        "https://www.chefkoch.de"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            Some(query) => {
                format!(
                    "{}/rs/s0/{}/Rezepte.html",
                    self.base_url(),
                    urlencoding::encode(query)
                )
            }
            // "Was koche ich heute?" (what should I cook today) — random/popular recipes
            None => format!("{}/rezepte/", self.base_url()),
        }
    }

    fn link_selector(&self) -> &'static str {
        "a[href*=\"/rezepte/\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        // Recipe URLs: chefkoch.de/rezepte/{numeric_id}/{Slug}.html[?query_params]
        let url_no_query = url.split('?').next().unwrap_or(url);
        let path = url_no_query
            .strip_prefix("https://www.chefkoch.de/rezepte/")
            .unwrap_or("");
        if path.is_empty() || !path.ends_with(".html") {
            return false;
        }
        // Must start with numeric ID segment
        path.split('/').next().is_some_and(|segment| {
            !segment.is_empty() && segment.chars().all(|c| c.is_ascii_digit())
        })
    }

    fn language(&self) -> &'static str {
        "de"
    }
}

/// Provider for foodnetwork.co.uk (English).
pub struct FoodNetworkUk;

impl RecipeProvider for FoodNetworkUk {
    fn name(&self) -> &'static str {
        "foodnetwork.co.uk"
    }

    fn base_url(&self) -> &'static str {
        "https://foodnetwork.co.uk"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            // Search results are Inertia.js JSON, not HTML links — listing only for now
            Some(_) | None => format!("{}/recipes", self.base_url()),
        }
    }

    fn link_selector(&self) -> &'static str {
        "a[href*=\"/recipes/\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        let path = url
            .strip_prefix("https://foodnetwork.co.uk/recipes/")
            .unwrap_or("");
        let slug = path.trim_end_matches('/');
        // Must be a single-segment slug, not empty, no nested paths
        !slug.is_empty() && !slug.contains('/')
    }

    fn language(&self) -> &'static str {
        "en"
    }
}

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
            // Billa search needs the raw query with accents (kuřecí, not kureci)
            Some(query) => {
                format!(
                    "{}/recepty?term={}",
                    self.base_url(),
                    urlencoding::encode(query)
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
            && !slug.contains('?')
            && !slug.starts_with("kategorie")
    }

    fn requires_browser(&self) -> bool {
        true
    }

    fn wait_condition(&self) -> WaitCondition {
        WaitCondition::Selector("a[href*='/recepty/']")
    }
}

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
            // Albert search: GraphQL-powered, needs raw query with accents
            Some(query) => {
                let encoded = urlencoding::encode(query);
                format!(
                    "{}/recepty/vyhledavani/?q={encoded}%3Arelevance&text={encoded}&sort=relevance",
                    self.base_url(),
                )
            }
            None => format!("{}/recepty", self.base_url()),
        }
    }

    fn link_selector(&self) -> &'static str {
        // Albert recipe URLs: /recepty/{slug}/r/{id}
        "a[href*=\"/r/\"]"
    }

    fn is_recipe_url(&self, url: &str) -> bool {
        // Recipe URLs: /recepty/{slug}/r/{id}
        let path = url
            .strip_prefix("https://www.albert.cz/recepty/")
            .unwrap_or("");
        // Must contain /r/ followed by an ID
        path.contains("/r/") && !path.starts_with("vyhledavani")
    }

    fn requires_browser(&self) -> bool {
        true
    }

    fn wait_condition(&self) -> WaitCondition {
        // Wait for recipe result links to render (GraphQL-powered, async)
        WaitCondition::Selector("a[href*='/r/']")
    }
}

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
                format!("{}/?s={}", self.base_url(), urlencoding::encode(query))
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

/// All available recipe providers.
pub fn providers() -> Vec<Box<dyn RecipeProvider>> {
    vec![
        Box::new(FreshIprima),
        Box::new(KuchyneLidlu),
        Box::new(ReceptyOdAnicky),
        Box::new(TopRecepty),
        Box::new(ApetitOnline),
        Box::new(ReceptyCz),
        Box::new(KauflandCz),
        Box::new(ReceptyNaKazdyDen),
        Box::new(BillaCz),
        Box::new(AlbertCz),
        Box::new(VareniCz),
        Box::new(Chefkoch),
        Box::new(FoodNetworkUk),
        Box::new(BbcGoodFood),
        // BudgetBytes excluded: Cloudflare JS challenge blocks headless Chrome
    ]
}

/// Check if a URL needs a browser and return its wait condition.
///
/// Uses the host portion of the URL (not a substring of the full URL) to
/// match against provider names, avoiding false positives from path segments.
pub fn browser_wait_condition(url: &str) -> Option<WaitCondition> {
    let host = url
        .split("://")
        .nth(1)
        .and_then(|s| s.split('/').next())
        .unwrap_or("");
    providers()
        .into_iter()
        .find(|p| {
            p.requires_browser() && (host == p.name() || host.ends_with(&format!(".{}", p.name())))
        })
        .map(|p| p.wait_condition())
}

/// Check if a URL belongs to a site that requires a headless browser.
///
/// Derives the answer from providers -- no hardcoded domain list.
pub fn needs_browser(url: &str) -> bool {
    browser_wait_condition(url).is_some()
}

/// Common URL filters shared across all providers.
///
/// Rejects listing pages, category pages, and URLs that are too short
/// to be individual recipe pages.
fn passes_common_filters(url: &str, base_url: &str) -> bool {
    if url.ends_with("/recepty") || url.ends_with("/recepty/") {
        return false;
    }
    if url.contains("/kategorie/") || url.contains("/category/") || url.contains("/vyhledavani") {
        return false;
    }
    if url.len() <= base_url.len() + 5 {
        return false;
    }
    true
}

/// Fetch a listing/search page and extract recipe URLs.
pub async fn fetch_recipe_urls(
    client: &reqwest::Client,
    browser: Option<&chromiumoxide::Browser>,
    provider: &dyn RecipeProvider,
    prompt: Option<&str>,
    max_urls: usize,
) -> Result<Vec<String>, String> {
    let url = provider.listing_url(prompt);

    let html = if provider.requires_browser() {
        let browser = browser
            .ok_or_else(|| format!("{}: requires browser but none available", provider.name()))?;
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

/// Simplify a conversational Czech query to keywords for recipe search engines.
///
/// Examples: "neco s rybou" -> "rybou", "rychla vecere s kuretem" -> "rychla vecere kurete"
fn simplify_query(query: &str) -> String {
    let stopwords = [
        "něco s ",
        "něco na ",
        "něco z ",
        "recept na ",
        "recept s ",
        "recept z ",
        "chci ",
        "chtěla bych ",
        "chtěl bych ",
        "dej mi ",
        "najdi ",
        "jaký ",
        "jaká ",
        "jaké ",
    ];
    let mut q = query.to_string();
    for sw in &stopwords {
        q = q.replace(sw, "");
    }

    // Remove common prepositions that confuse search
    let prepositions = [" s ", " z ", " na ", " pro ", " bez ", " od "];
    for prep in &prepositions {
        q = q.replace(prep, " ");
    }

    q.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_recipe_url: fresh.iprima.cz ---

    #[test]
    fn fresh_accepts_recipe_with_6digit_id() {
        let provider = FreshIprima;
        assert!(
            provider.is_recipe_url("https://fresh.iprima.cz/nadychana-omeleta-se-syrem-506517",)
        );
    }

    #[test]
    fn fresh_accepts_another_recipe() {
        let provider = FreshIprima;
        assert!(
            provider.is_recipe_url("https://fresh.iprima.cz/chlebovy-nakyp-s-klobasou-508979",)
        );
    }

    #[test]
    fn fresh_rejects_listing_page() {
        let provider = FreshIprima;
        assert!(!provider.is_recipe_url("https://fresh.iprima.cz/recepty"));
    }

    #[test]
    fn fresh_rejects_subcategory() {
        let provider = FreshIprima;
        assert!(!provider.is_recipe_url("https://fresh.iprima.cz/recepty/maso",));
    }

    #[test]
    fn fresh_rejects_chef_page() {
        let provider = FreshIprima;
        assert!(!provider.is_recipe_url("https://fresh.iprima.cz/kuchari/zdenek-pohlreich",));
    }

    #[test]
    fn fresh_rejects_collection() {
        let provider = FreshIprima;
        assert!(!provider.is_recipe_url("https://fresh.iprima.cz/specialy/vejce",));
    }

    #[test]
    fn fresh_rejects_listicle_without_numeric_id() {
        let provider = FreshIprima;
        assert!(!provider.is_recipe_url("https://fresh.iprima.cz/20-tipu-na-rychle-vecere",));
    }

    // --- is_recipe_url: kuchynelidlu.cz ---

    #[test]
    fn lidl_accepts_recipe() {
        let provider = KuchyneLidlu;
        assert!(
            provider
                .is_recipe_url("https://kuchynelidlu.cz/recept/avokadovy-salat-s-kurecim-masem",)
        );
    }

    #[test]
    fn lidl_accepts_another_recipe() {
        let provider = KuchyneLidlu;
        assert!(provider.is_recipe_url("https://kuchynelidlu.cz/recept/pad-thai-nudle",));
    }

    #[test]
    fn lidl_rejects_howto_article() {
        let provider = KuchyneLidlu;
        assert!(
            !provider.is_recipe_url("https://kuchynelidlu.cz/recept/jak-pouzivat-tlakovy-hrnec",)
        );
    }

    #[test]
    fn lidl_rejects_listing() {
        let provider = KuchyneLidlu;
        assert!(!provider.is_recipe_url("https://kuchynelidlu.cz/recepty"));
    }

    // --- is_recipe_url: receptyodanicky.cz ---

    #[test]
    fn anicky_accepts_recipe() {
        let provider = ReceptyOdAnicky;
        assert!(
            provider.is_recipe_url("https://www.receptyodanicky.cz/kure-ve-sladkokysele-omacce/",)
        );
    }

    #[test]
    fn anicky_accepts_another_recipe() {
        let provider = ReceptyOdAnicky;
        assert!(provider.is_recipe_url("https://www.receptyodanicky.cz/cottage-palacinky/",));
    }

    #[test]
    fn anicky_rejects_author() {
        let provider = ReceptyOdAnicky;
        assert!(!provider.is_recipe_url("https://www.receptyodanicky.cz/author/anicka/",));
    }

    #[test]
    fn anicky_rejects_category() {
        let provider = ReceptyOdAnicky;
        assert!(
            !provider
                .is_recipe_url("https://www.receptyodanicky.cz/category/recepty/hlavni-chod/",)
        );
    }

    #[test]
    fn anicky_rejects_vsechny_recepty() {
        let provider = ReceptyOdAnicky;
        assert!(!provider.is_recipe_url("https://www.receptyodanicky.cz/vsechny-recepty/",));
    }

    #[test]
    fn anicky_rejects_about_page() {
        let provider = ReceptyOdAnicky;
        assert!(!provider.is_recipe_url("https://www.receptyodanicky.cz/o-mne/",));
    }

    #[test]
    fn anicky_rejects_spoluprace() {
        let provider = ReceptyOdAnicky;
        assert!(!provider.is_recipe_url("https://www.receptyodanicky.cz/spoluprace/",));
    }

    #[test]
    fn anicky_rejects_newsletter() {
        let provider = ReceptyOdAnicky;
        assert!(!provider.is_recipe_url("https://www.receptyodanicky.cz/newsletter/",));
    }

    #[test]
    fn anicky_rejects_shop_subdomain() {
        let provider = ReceptyOdAnicky;
        assert!(!provider.is_recipe_url("https://shop.receptyodanicky.cz/na-kazdy-den/",));
    }

    // --- is_recipe_url: toprecepty.cz ---

    #[test]
    fn toprecepty_accepts_recipe_with_numeric_id() {
        let provider = TopRecepty;
        assert!(provider.is_recipe_url("https://www.toprecepty.cz/recept/28963-kure-na-paprice/",));
    }

    #[test]
    fn toprecepty_accepts_another_recipe() {
        let provider = TopRecepty;
        assert!(provider.is_recipe_url("https://www.toprecepty.cz/recept/11511-palacinky/",));
    }

    #[test]
    fn toprecepty_rejects_listing() {
        let provider = TopRecepty;
        assert!(!provider.is_recipe_url("https://www.toprecepty.cz/recepty/"));
    }

    // --- is_recipe_url: apetitonline.cz ---

    #[test]
    fn apetit_accepts_recipe() {
        let provider = ApetitOnline;
        assert!(
            provider.is_recipe_url("https://www.apetitonline.cz/recept/kureci-balicky-s-fetou",)
        );
    }

    #[test]
    fn apetit_accepts_another_recipe() {
        let provider = ApetitOnline;
        assert!(
            provider.is_recipe_url("https://www.apetitonline.cz/recept/dokonaly-domaci-hamburger",)
        );
    }

    #[test]
    fn apetit_rejects_empty_recept_path() {
        let provider = ApetitOnline;
        assert!(!provider.is_recipe_url("https://www.apetitonline.cz/recept/"));
    }

    #[test]
    fn apetit_rejects_pagination() {
        let provider = ApetitOnline;
        assert!(!provider.is_recipe_url("https://www.apetitonline.cz/recept/?page=2",));
    }

    // --- is_recipe_url: recepty.cz ---

    #[test]
    fn recepty_cz_accepts_recipe() {
        let provider = ReceptyCz;
        assert!(provider.is_recipe_url("https://www.recepty.cz/recept/kure-palivec-6056"));
    }

    #[test]
    fn recepty_cz_accepts_another_recipe() {
        let provider = ReceptyCz;
        assert!(provider.is_recipe_url(
            "https://www.recepty.cz/recept/salat-z-rimskeho-salatu-s-kurecim-prsem-12345"
        ));
    }

    #[test]
    fn recepty_cz_rejects_listing() {
        let provider = ReceptyCz;
        assert!(!provider.is_recipe_url("https://www.recepty.cz/recepty"));
    }

    #[test]
    fn recepty_cz_rejects_slug_without_id() {
        let provider = ReceptyCz;
        assert!(!provider.is_recipe_url("https://www.recepty.cz/recept/kure-palivec"));
    }

    #[test]
    fn recepty_cz_rejects_search() {
        let provider = ReceptyCz;
        assert!(!provider.is_recipe_url("https://www.recepty.cz/vyhledavani?text=kure"));
    }

    // --- is_recipe_url: kaufland.cz ---

    #[test]
    fn kaufland_accepts_recipe() {
        let provider = KauflandCz;
        assert!(provider.is_recipe_url(
            "https://prodejny.kaufland.cz/recepty/vyhledat-recept/recept.pecena-kureci-stehna.r_id=CZ_1600.html"
        ));
    }

    #[test]
    fn kaufland_accepts_recipe_with_recipe_id() {
        let provider = KauflandCz;
        assert!(provider.is_recipe_url(
            "https://prodejny.kaufland.cz/recepty/vyhledat-recept/recept.pad-thai.r_id=Recipe_12345.html"
        ));
    }

    #[test]
    fn kaufland_rejects_listing() {
        let provider = KauflandCz;
        assert!(!provider.is_recipe_url("https://prodejny.kaufland.cz/recepty.html"));
    }

    #[test]
    fn kaufland_rejects_category() {
        let provider = KauflandCz;
        assert!(!provider.is_recipe_url("https://prodejny.kaufland.cz/recepty/hlavni-jidla.html"));
    }

    // --- is_recipe_url: receptynakazdyden.cz ---

    #[test]
    fn rnakazdyden_accepts_recipe() {
        let provider = ReceptyNaKazdyDen;
        assert!(provider.is_recipe_url("https://www.receptynakazdyden.cz/recept/kure-na-paprice/"));
    }

    #[test]
    fn rnakazdyden_accepts_recipe_without_trailing_slash() {
        let provider = ReceptyNaKazdyDen;
        assert!(
            provider.is_recipe_url("https://www.receptynakazdyden.cz/recept/kureci-cina-s-cuketou")
        );
    }

    #[test]
    fn rnakazdyden_rejects_listing() {
        let provider = ReceptyNaKazdyDen;
        assert!(!provider.is_recipe_url("https://www.receptynakazdyden.cz/recepty/hlavni-jidlo/"));
    }

    #[test]
    fn rnakazdyden_rejects_category() {
        let provider = ReceptyNaKazdyDen;
        assert!(!provider.is_recipe_url("https://www.receptynakazdyden.cz/category/recepty/"));
    }

    #[test]
    fn rnakazdyden_rejects_sponsored() {
        let provider = ReceptyNaKazdyDen;
        assert!(!provider.is_recipe_url("https://www.receptynakazdyden.cz/hellmanns/"));
    }

    // --- is_recipe_url: chefkoch.de ---

    #[test]
    fn chefkoch_accepts_recipe() {
        let provider = Chefkoch;
        assert!(provider.is_recipe_url(
            "https://www.chefkoch.de/rezepte/472271140790423/Toskanischer-Haehnchen-Auflauf.html"
        ));
    }

    #[test]
    fn chefkoch_accepts_another_recipe() {
        let provider = Chefkoch;
        assert!(
            provider.is_recipe_url("https://www.chefkoch.de/rezepte/1234567/Kartoffelsuppe.html")
        );
    }

    #[test]
    fn chefkoch_accepts_recipe_with_query_params() {
        let provider = Chefkoch;
        assert!(provider.is_recipe_url(
            "https://www.chefkoch.de/rezepte/4193061674035989/Mediterranes-Gulasch.html?ck_source=search-recipe&ck_element=recipe_search_list"
        ));
    }

    #[test]
    fn chefkoch_rejects_listing() {
        let provider = Chefkoch;
        assert!(!provider.is_recipe_url("https://www.chefkoch.de/rezepte/"));
    }

    #[test]
    fn chefkoch_rejects_suggestion_page() {
        let provider = Chefkoch;
        assert!(!provider.is_recipe_url("https://www.chefkoch.de/rezepte/was-koche-ich-heute/"));
    }

    #[test]
    fn chefkoch_language_is_german() {
        assert_eq!(Chefkoch.language(), "de");
    }

    // --- is_recipe_url: foodnetwork.co.uk ---

    #[test]
    fn foodnetwork_accepts_recipe() {
        let provider = FoodNetworkUk;
        assert!(provider.is_recipe_url("https://foodnetwork.co.uk/recipes/chicken-tikka-masala"));
    }

    #[test]
    fn foodnetwork_accepts_recipe_with_trailing_slash() {
        let provider = FoodNetworkUk;
        assert!(provider.is_recipe_url("https://foodnetwork.co.uk/recipes/chicken-katsu-bowl/"));
    }

    #[test]
    fn foodnetwork_rejects_listing() {
        let provider = FoodNetworkUk;
        assert!(!provider.is_recipe_url("https://foodnetwork.co.uk/recipes/"));
    }

    #[test]
    fn foodnetwork_rejects_nested_path() {
        let provider = FoodNetworkUk;
        assert!(
            !provider.is_recipe_url("https://foodnetwork.co.uk/recipes/collection/quick-dinners/")
        );
    }

    #[test]
    fn foodnetwork_language_is_english() {
        assert_eq!(FoodNetworkUk.language(), "en");
    }

    // --- language defaults ---

    #[test]
    fn czech_providers_default_to_cs() {
        assert_eq!(FreshIprima.language(), "cs");
        assert_eq!(KuchyneLidlu.language(), "cs");
        assert_eq!(ReceptyCz.language(), "cs");
        assert_eq!(KauflandCz.language(), "cs");
        assert_eq!(ReceptyNaKazdyDen.language(), "cs");
    }

    // --- simplify_query ---

    #[test]
    fn simplify_strips_neco_s() {
        assert_eq!(simplify_query("něco s rybou"), "rybou");
    }

    #[test]
    fn simplify_keeps_plain_keywords() {
        assert_eq!(simplify_query("rychlá večeře"), "rychlá večeře");
    }

    #[test]
    fn simplify_strips_recept_na() {
        assert_eq!(simplify_query("recept na kuře"), "kuře");
    }

    #[test]
    fn simplify_strips_chci_and_preposition() {
        assert_eq!(simplify_query("chci něco z těstovin"), "těstovin");
    }

    #[test]
    fn simplify_strips_najdi() {
        assert_eq!(simplify_query("najdi polévku"), "polévku");
    }

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
        assert!(!p.is_recipe_url("https://www.billa.cz/recepty?term=kure"));
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

    // --- is_recipe_url: albert.cz ---

    #[test]
    fn albert_accepts_recipe() {
        let p = AlbertCz;
        assert!(p.is_recipe_url("https://www.albert.cz/recepty/bbq-kure/r/a2805"));
    }

    #[test]
    fn albert_accepts_recipe_with_trailing_slash() {
        let p = AlbertCz;
        assert!(p.is_recipe_url("https://www.albert.cz/recepty/kureci-nudlicky/r/a1234/"));
    }

    #[test]
    fn albert_rejects_listing() {
        let p = AlbertCz;
        assert!(!p.is_recipe_url("https://www.albert.cz/recepty"));
    }

    #[test]
    fn albert_rejects_search() {
        let p = AlbertCz;
        assert!(!p.is_recipe_url("https://www.albert.cz/recepty/vyhledavani/?q=kure%3Arelevance"));
    }

    #[test]
    fn albert_requires_browser() {
        assert!(AlbertCz.requires_browser());
    }

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
        assert!(!p.is_recipe_url(
            "https://www.bbcgoodfood.com/recipes/category/special-occasion-collections"
        ));
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
}
