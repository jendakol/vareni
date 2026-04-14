//! Site scraping: fetch recipe listing pages and extract recipe URLs.
//!
//! Each supported recipe site is represented by a [`RecipeProvider`] implementation
//! that encapsulates site-specific URL validation, search URL construction, and
//! CSS selectors.

use scraper::{Html, Selector};

/// A recipe provider that can list and validate recipe URLs from a specific site.
pub trait RecipeProvider: Send + Sync {
    /// Human-readable site name (e.g. "fresh.iprima.cz").
    fn name(&self) -> &str;

    /// Base URL with scheme, no trailing slash (e.g. "https://fresh.iprima.cz").
    fn base_url(&self) -> &str;

    /// Build the full URL to fetch for listing or search.
    ///
    /// When `prompt` is `Some`, the provider should return a search URL if supported,
    /// falling back to the listing URL otherwise.
    fn listing_url(&self, prompt: Option<&str>) -> String;

    /// CSS selector for extracting links from the listing page.
    fn link_selector(&self) -> &str;

    /// Is this URL a valid individual recipe page (not a listing, category, author, etc.)?
    fn is_recipe_url(&self, url: &str) -> bool;
}

/// Provider for fresh.iprima.cz.
pub struct FreshIprima;

impl RecipeProvider for FreshIprima {
    fn name(&self) -> &str {
        "fresh.iprima.cz"
    }

    fn base_url(&self) -> &str {
        "https://fresh.iprima.cz"
    }

    fn listing_url(&self, _prompt: Option<&str>) -> String {
        // No search support
        format!("{}/recepty", self.base_url())
    }

    fn link_selector(&self) -> &str {
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
    fn name(&self) -> &str {
        "kuchynelidlu.cz"
    }

    fn base_url(&self) -> &str {
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

    fn link_selector(&self) -> &str {
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
    fn name(&self) -> &str {
        "receptyodanicky.cz"
    }

    fn base_url(&self) -> &str {
        "https://www.receptyodanicky.cz"
    }

    fn listing_url(&self, _prompt: Option<&str>) -> String {
        // No search support
        format!("{}/", self.base_url())
    }

    fn link_selector(&self) -> &str {
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
    fn name(&self) -> &str {
        "toprecepty.cz"
    }

    fn base_url(&self) -> &str {
        "https://www.toprecepty.cz"
    }

    fn listing_url(&self, prompt: Option<&str>) -> String {
        match prompt {
            Some(query) => {
                let keywords = simplify_query(query);
                format!(
                    "{}/vyhledavani/?q={}",
                    self.base_url(),
                    urlencoding::encode(&keywords)
                )
            }
            None => format!("{}/recepty/", self.base_url()),
        }
    }

    fn link_selector(&self) -> &str {
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
    fn name(&self) -> &str {
        "apetitonline.cz"
    }

    fn base_url(&self) -> &str {
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

    fn link_selector(&self) -> &str {
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

/// All available recipe providers.
pub fn providers() -> Vec<Box<dyn RecipeProvider>> {
    vec![
        Box::new(FreshIprima),
        Box::new(KuchyneLidlu),
        Box::new(ReceptyOdAnicky),
        Box::new(TopRecepty),
        Box::new(ApetitOnline),
    ]
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
    provider: &dyn RecipeProvider,
    prompt: Option<&str>,
    max_urls: usize,
) -> Result<Vec<String>, String> {
    let url = provider.listing_url(prompt);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("{}: {e}", provider.name()))?;

    if !resp.status().is_success() {
        return Err(format!("{}: HTTP {}", provider.name(), resp.status()));
    }

    let html = resp
        .text()
        .await
        .map_err(|e| format!("{}: failed to read body: {e}", provider.name()))?;

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
}
