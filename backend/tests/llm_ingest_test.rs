//! End-to-end LLM ingest acceptance test.
//!
//! Run with: cargo test --release --ignored --test llm_ingest_test -- --nocapture
//! Requires ANTHROPIC_API_KEY in .env at the repo root.

mod common;

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY and network; run manually"]
async fn parses_multi_section_recipe_from_vareni_cz() {
    // Load .env from repo root (one level up from backend/)
    let _ = dotenvy::from_path(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.env"));

    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY missing");

    let client = cooking_app::ai::client::AnthropicClient::new(&api_key);
    let http_client = reqwest::Client::new();

    let url = "https://www.vareni.cz/recepty/svestkovy-kolac-s-tvarohem/";

    // Use the production parse_url path (browser=None falls back to reqwest fetch,
    // then JSON-LD extraction + HTML text fallback, then AI parse).
    let parsed = cooking_app::ai::ingest::parse_url(&client, &http_client, None, url)
        .await
        .expect("parse_url failed");

    eprintln!("=== ParsedRecipe ===");
    eprintln!("title: {}", parsed.title);
    eprintln!("description: {:?}", parsed.description);
    eprintln!("servings: {:?}", parsed.servings);
    eprintln!("tags: {:?}", parsed.tags);
    for (i, s) in parsed.sections.iter().enumerate() {
        eprintln!(
            "  section[{}] label={:?} ing_count={} step_count={} prep={:?} cook={:?}",
            i,
            s.label,
            s.ingredients.len(),
            s.steps.len(),
            s.prep_time_min,
            s.cook_time_min
        );
        for ing in &s.ingredients {
            eprintln!(
                "    - {} {} {} (note: {:?})",
                ing.amount.unwrap_or(0.0),
                ing.unit.as_deref().unwrap_or(""),
                ing.name,
                ing.note
            );
        }
        for step in &s.steps {
            eprintln!(
                "    {}. {}",
                step.step_order,
                step.instruction.chars().take(60).collect::<String>()
            );
        }
    }

    // Acceptance assertions
    assert!(
        parsed.sections.len() >= 3,
        "expected >=3 sections (testo/napln/drobenka), got {}",
        parsed.sections.len()
    );

    // Section labels should mention the three parts (case-insensitive, allowing "Na testo" etc.)
    let label_text = parsed
        .sections
        .iter()
        .filter_map(|s| s.label.as_deref())
        .collect::<Vec<_>>()
        .join(" | ")
        .to_lowercase();
    assert!(
        label_text.contains("těsto"),
        "no section labelled 'těsto'; labels: {label_text}"
    );
    assert!(
        label_text.contains("náplň")
            || label_text.contains("naplň")
            || label_text.contains("nápln"),
        "no filling section; labels: {label_text}"
    );
    assert!(
        label_text.contains("drobenk"),
        "no streusel section; labels: {label_text}"
    );

    // Each section should have content (at least one ingredient or step)
    for (i, s) in parsed.sections.iter().enumerate() {
        assert!(
            !s.ingredients.is_empty() || !s.steps.is_empty(),
            "section[{}] '{}' is empty — likely a hallucinated heading like 'Tip'",
            i,
            s.label.as_deref().unwrap_or("")
        );
    }
}
