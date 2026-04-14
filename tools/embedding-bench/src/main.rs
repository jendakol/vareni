use std::path::PathBuf;
use std::sync::Mutex;

use comfy_table::{Cell, Color, Table};
use ndarray::{Array2, Axis};
use ort::session::Session;
use ort::value::Tensor;
use serde::Deserialize;

// --- Data types ---

#[derive(Debug, Deserialize)]
struct Recipe {
    id: String,
    title: String,
    description: Option<String>,
    tags: Option<Vec<String>>,
    ingredients: Option<Vec<String>>,
    #[serde(rename = "_note")]
    note: Option<String>,
}

// --- Summary templates ---

/// Template A: mechanical concatenation — "Title. Tags: a, b. Ingredience: x, y, z."
fn summary_mechanical(r: &Recipe) -> String {
    let mut parts = vec![r.title.clone()];
    if let Some(tags) = &r.tags {
        if !tags.is_empty() {
            parts.push(format!("Tagy: {}", tags.join(", ")));
        }
    }
    if let Some(ings) = &r.ingredients {
        if !ings.is_empty() {
            parts.push(format!("Ingredience: {}", ings.join(", ")));
        }
    }
    parts.join(". ") + "."
}

/// Template B: natural sentence — "Title je recept (popis). Obsahuje: x, y, z."
fn summary_natural(r: &Recipe) -> String {
    let desc = r
        .description
        .as_deref()
        .filter(|d| !d.is_empty())
        .map(|d| format!(" — {d}"))
        .unwrap_or_default();

    let tags = r
        .tags
        .as_ref()
        .filter(|t| !t.is_empty())
        .map(|t| format!(" Kategorie: {}.", t.join(", ")))
        .unwrap_or_default();

    let ings = r
        .ingredients
        .as_ref()
        .filter(|i| !i.is_empty())
        .map(|i| format!(" Obsahuje: {}.", i.join(", ")))
        .unwrap_or_default();

    format!("{}{}.{}{}", r.title, desc, tags, ings)
}

/// Template C: title + tags only (no ingredients)
fn summary_title_tags(r: &Recipe) -> String {
    let tags = r
        .tags
        .as_ref()
        .filter(|t| !t.is_empty())
        .map(|t| format!(". {}", t.join(", ")))
        .unwrap_or_default();
    format!("{}{}", r.title, tags)
}

/// Template D: title only
fn summary_title_only(r: &Recipe) -> String {
    r.title.clone()
}

// --- Title normalization ---

/// Mechanical title cleanup: strip attributions, adjectives, extra descriptions
fn normalize_title_mechanical(title: &str) -> String {
    let mut t = title.to_string();
    // Remove "podle X" attributions
    if let Some(pos) = t.find(" podle ") {
        t.truncate(pos);
    }
    // Remove "z X" source (e.g. "z Rohlíku") — only after last main word
    // Be careful: "z paprikových lusků" is meaningful, "z Rohlíku" is not
    // Only strip "z <Capitalized>" patterns (proper nouns)
    let re_patterns = [
        " z Rohlíku", " z Lidlu", " od Aničky",
    ];
    for pat in &re_patterns {
        t = t.replace(pat, "");
    }
    // Remove common adjective prefixes
    let adj_prefixes = [
        "Jednoduché ", "Jednoduchý ", "Jednoduchá ",
        "Rychlé ", "Rychlý ", "Rychlá ",
        "Superrychlé ", "Superrychle ",
        "Domácí ",
        "Tradiční ",
        "Klasické ", "Klasický ", "Klasická ",
    ];
    for prefix in &adj_prefixes {
        if t.starts_with(prefix) {
            t = t[prefix.len()..].to_string();
        }
    }
    // Remove parenthetical explanations like "(marry me chicken)" or "(ke kachně nebo huse)"
    if let Some(paren_start) = t.find('(') {
        if let Some(paren_end) = t.find(')') {
            if paren_end > paren_start {
                let before = t[..paren_start].trim_end().to_string();
                let after = t[paren_end + 1..].to_string();
                t = format!("{before}{after}");
            }
        }
    }
    t.trim().to_string()
}

/// Simulated LLM normalization — what Claude would produce as a canonical dish name.
/// Hardcoded for benchmark; in production this would be a Haiku call.
fn normalize_title_llm(title: &str) -> String {
    let map: &[(&str, &str)] = &[
        // Real recipes
        ("V troubě pomalu pečená kachna", "pečená kachna"),
        ("Mac'n'cheese z Rohlíku", "mac and cheese"),
        ("Mac'n'cheese superrychle jednoduše", "mac and cheese"),
        ("Jednoduché kuře na paprice s rýží", "kuře na paprice"),
        ("Kuře na paprice podle Pauluse", "kuře na paprice"),
        ("Rajčatovo-zeleninová omáčka s těstovinami", "zeleninové těstoviny"),
        ("Kuřecí kousky s těstovinami a smetanovo-sýrovou omáčkou", "kuřecí těstoviny se smetanou"),
        ("Orzo s kuřecím masem na smetaně (marry me chicken)", "kuřecí těstoviny se smetanou"),
        ("Hovězí maso na rajčatovo-worcesterové omáčce s bramborovou kaší", "hovězí na rajčatové omáčce"),
        ("Tagliatelle s kousky lososa", "těstoviny s lososem"),
        ("Těstovinový salát", "těstovinový salát"),
        ("Kapr", "pečený kapr"),
        ("Bramborový salát ke kaprovi", "bramborový salát"),
        ("Grilovaný hermelín v troubě", "grilovaný hermelín"),
        ("Omáčka k rybě (máslovo-česneko-citronová)", "máslová omáčka k rybě"),
        ("Rizoto se sýrem a brokolicí", "sýrové rizoto s brokolicí"),
        ("Salát s listovým špenátem a sýrem feta", "špenátový salát s fetou"),
        ("Ovar z vepřové plece nebo srdce s chlebem a křenem", "ovar"),
        ("Chlupaté knedlíky (ke kachně nebo huse)", "chlupaté knedlíky"),
        ("Dýňová polévka na sladko", "dýňová krémová polévka"),
        ("Francouzské brambory", "francouzské brambory"),
        ("Asijská vaječná rýže s krevetami", "smažená rýže s krevetami"),
        ("Velikonoční nádivka s kopřivami", "nádivka s kopřivami"),
        ("Těstoviny ze 3 ingrediencí (těstoviny alfredo)", "těstoviny alfredo"),
        ("Rizoto z paprikových lusků", "paprikové rizoto"),
        // Synthetic
        ("Makarony se sýrem", "mac and cheese"),
        ("Sýrové těstoviny pro děti", "mac and cheese"),
        ("Kuřecí paprikáš", "kuře na paprice"),
        ("Chicken paprikash", "kuře na paprice"),
        ("Pečená kachna s knedlíky a zelím", "pečená kachna"),
        ("Pad Thai s krevetami", "pad thai s krevetami"),
        ("Hovězí guláš s houskovými knedlíky", "hovězí guláš"),
        ("Svíčková na smetaně", "svíčková na smetaně"),
        ("Risotto alla milanese", "šafránové rizoto"),
        ("Řecký salát", "řecký salát"),
        ("Špenátový salát s fetou a rajčaty", "špenátový salát s fetou"),
        ("Losos na másle s citronem", "losos na másle"),
        // Scraped
        ("Krémové těstoviny s kuřecím masem a parmazánem", "kuřecí těstoviny se smetanou"),
        ("Grilovaný hermelín zabalený v šunce s pikantními fazolemi", "grilovaný hermelín"),
        ("Zapečené lasagne s mletým masem a mangoldem", "masové lasagne"),
        ("Smažená ryba s kurkumou a koprem", "smažená ryba s kurkumou"),
        ("Srbské rizoto", "srbské rizoto"),
        ("Zeleninové lasagne", "zeleninové lasagne"),
        ("Pečená pražma s medem a sezamem", "pečená pražma"),
        ("Kuře ve sladkokyselé omáčce", "kuře ve sladkokyselé omáčce"),
        ("Krémová květáková polévka", "květáková krémová polévka"),
        ("Pečené plněné brambory", "plněné brambory"),
    ];
    map.iter()
        .find(|(orig, _)| *orig == title)
        .map(|(_, norm)| norm.to_string())
        .unwrap_or_else(|| title.to_string())
}

/// Template H: LLM-normalized title + title-heavy approach (top5 filtered ingredients)
fn summary_llm_normalized(r: &Recipe) -> String {
    let norm_title = normalize_title_llm(&r.title);

    let tags = r
        .tags
        .as_ref()
        .filter(|t| !t.is_empty())
        .map(|t| format!(" Kategorie: {}.", t.join(", ")))
        .unwrap_or_default();

    let ings = r
        .ingredients
        .as_ref()
        .filter(|i| !i.is_empty())
        .map(|i| {
            let filtered: Vec<&str> = i.iter()
                .map(|s| s.as_str())
                .filter(|s| !is_stop_ingredient(s))
                .take(5)
                .collect();
            if filtered.is_empty() { String::new() } else { format!(" Obsahuje: {}.", filtered.join(", ")) }
        })
        .unwrap_or_default();

    format!("{}. {}.{}{}", norm_title, norm_title, tags, ings)
}

/// Template I: mechanical title cleanup + title-heavy + top5
fn summary_mech_normalized(r: &Recipe) -> String {
    let norm_title = normalize_title_mechanical(&r.title);

    let tags = r
        .tags
        .as_ref()
        .filter(|t| !t.is_empty())
        .map(|t| format!(" Kategorie: {}.", t.join(", ")))
        .unwrap_or_default();

    let ings = r
        .ingredients
        .as_ref()
        .filter(|i| !i.is_empty())
        .map(|i| {
            let filtered: Vec<&str> = i.iter()
                .map(|s| s.as_str())
                .filter(|s| !is_stop_ingredient(s))
                .take(5)
                .collect();
            if filtered.is_empty() { String::new() } else { format!(" Obsahuje: {}.", filtered.join(", ")) }
        })
        .unwrap_or_default();

    format!("{}. {}.{}{}", norm_title, norm_title, tags, ings)
}

// Common cooking ingredients that appear in most recipes and carry no signal
const STOP_INGREDIENTS: &[&str] = &[
    "sůl", "pepř", "sůl a pepř", "pepř a sůl", "olej", "olivový olej", "rostlinný olej",
    "česnek", "cibule", "voda", "máslo", "smetana", "černý pepř", "bílý pepř",
    "mletý pepř", "řepkový olej", "neutrální olej",
];

fn is_stop_ingredient(ing: &str) -> bool {
    let lower = ing.to_lowercase();
    STOP_INGREDIENTS.iter().any(|s| lower == *s)
}

/// Template E: natural + stop-ingredient filtering
fn summary_filtered(r: &Recipe) -> String {
    let desc = r
        .description
        .as_deref()
        .filter(|d| !d.is_empty())
        .map(|d| format!(" — {d}"))
        .unwrap_or_default();

    let tags = r
        .tags
        .as_ref()
        .filter(|t| !t.is_empty())
        .map(|t| format!(" Kategorie: {}.", t.join(", ")))
        .unwrap_or_default();

    let ings = r
        .ingredients
        .as_ref()
        .filter(|i| !i.is_empty())
        .map(|i| {
            let filtered: Vec<&str> = i.iter()
                .map(|s| s.as_str())
                .filter(|s| !is_stop_ingredient(s))
                .collect();
            if filtered.is_empty() { String::new() } else { format!(" Obsahuje: {}.", filtered.join(", ")) }
        })
        .unwrap_or_default();

    format!("{}{}.{}{}", r.title, desc, tags, ings)
}

/// Template F: natural + only first 5 ingredients (main ones, no stop words)
fn summary_top5(r: &Recipe) -> String {
    let desc = r
        .description
        .as_deref()
        .filter(|d| !d.is_empty())
        .map(|d| format!(" — {d}"))
        .unwrap_or_default();

    let tags = r
        .tags
        .as_ref()
        .filter(|t| !t.is_empty())
        .map(|t| format!(" Kategorie: {}.", t.join(", ")))
        .unwrap_or_default();

    let ings = r
        .ingredients
        .as_ref()
        .filter(|i| !i.is_empty())
        .map(|i| {
            let filtered: Vec<&str> = i.iter()
                .map(|s| s.as_str())
                .filter(|s| !is_stop_ingredient(s))
                .take(5)
                .collect();
            if filtered.is_empty() { String::new() } else { format!(" Hlavní ingredience: {}.", filtered.join(", ")) }
        })
        .unwrap_or_default();

    format!("{}{}.{}{}", r.title, desc, tags, ings)
}

/// Template G: title repeated for emphasis + tags + filtered ingredients
fn summary_title_heavy(r: &Recipe) -> String {
    let tags = r
        .tags
        .as_ref()
        .filter(|t| !t.is_empty())
        .map(|t| format!(" Kategorie: {}.", t.join(", ")))
        .unwrap_or_default();

    let ings = r
        .ingredients
        .as_ref()
        .filter(|i| !i.is_empty())
        .map(|i| {
            let filtered: Vec<&str> = i.iter()
                .map(|s| s.as_str())
                .filter(|s| !is_stop_ingredient(s))
                .take(5)
                .collect();
            if filtered.is_empty() { String::new() } else { format!(" Obsahuje: {}.", filtered.join(", ")) }
        })
        .unwrap_or_default();

    // Repeat title to give it 2x weight in mean pooling
    format!("{}. {}.{}{}", r.title, r.title, tags, ings)
}

// --- ONNX embedding (adapted from second-brain) ---

struct Embedder {
    session: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
}

impl Embedder {
    fn new(model_dir: &PathBuf) -> Self {
        let model_path = model_dir.join("model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        let session = Session::builder()
            .expect("session builder")
            .with_intra_threads(1)
            .expect("intra threads")
            .commit_from_file(&model_path)
            .expect("load ONNX model");

        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).expect("load tokenizer");

        Self {
            session: Mutex::new(session),
            tokenizer,
        }
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let encoding = self.tokenizer.encode(text, true).expect("tokenize");
        let ids = encoding.get_ids();
        let mask = encoding.get_attention_mask();
        let type_ids = encoding.get_type_ids();
        let seq_len = ids.len();

        let input_ids =
            Array2::from_shape_vec((1, seq_len), ids.iter().map(|&x| i64::from(x)).collect())
                .unwrap();
        let attention_mask =
            Array2::from_shape_vec((1, seq_len), mask.iter().map(|&x| i64::from(x)).collect())
                .unwrap();
        let token_type_ids =
            Array2::from_shape_vec((1, seq_len), type_ids.iter().map(|&x| i64::from(x)).collect())
                .unwrap();

        let mut session = self.session.lock().unwrap();
        let outputs = session
            .run(ort::inputs![
                "input_ids" => Tensor::from_array(input_ids).unwrap(),
                "attention_mask" => Tensor::from_array(attention_mask).unwrap(),
                "token_type_ids" => Tensor::from_array(token_type_ids).unwrap(),
            ])
            .expect("ONNX inference");

        let (shape, hidden_data) = outputs[0].try_extract_tensor::<f32>().unwrap();
        let hidden_dim = *shape.last().unwrap() as usize;
        let hidden = ndarray::ArrayView2::from_shape((seq_len, hidden_dim), hidden_data).unwrap();

        // Mean pooling with attention mask
        let mask_f32: Array2<f32> =
            Array2::from_shape_vec((seq_len, 1), mask.iter().map(|&x| x as f32).collect())
                .unwrap();
        let masked = &hidden * &mask_f32;
        let summed = masked.sum_axis(Axis(0));
        let mask_sum = mask_f32.sum().max(1e-9);
        let pooled = &summed / mask_sum;

        // L2 normalize
        let norm = pooled.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
        pooled.iter().map(|x| x / norm).collect()
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

// --- Main ---

fn main() {
    let model_dir = std::env::var("EMBEDDING_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Try common locations
            let paths = [
                PathBuf::from("models/all-MiniLM-L6-v2"),
                PathBuf::from("../../models/all-MiniLM-L6-v2"),
                dirs().join("models/all-MiniLM-L6-v2"),
            ];
            paths
                .into_iter()
                .find(|p| p.join("model.onnx").exists())
                .expect(
                    "Model not found. Set EMBEDDING_MODEL_DIR or symlink models/all-MiniLM-L6-v2",
                )
        });

    println!("Loading model from: {}", model_dir.display());
    let embedder = Embedder::new(&model_dir);

    // Load recipes
    let real: Vec<Recipe> =
        serde_json::from_str(&std::fs::read_to_string("data/recipes.json").expect("read recipes"))
            .expect("parse recipes");
    let synthetic: Vec<Recipe> = serde_json::from_str(
        &std::fs::read_to_string("data/synthetic.json").expect("read synthetic"),
    )
    .expect("parse synthetic");
    let scraped: Vec<Recipe> = serde_json::from_str(
        &std::fs::read_to_string("data/scraped.json").expect("read scraped"),
    )
    .expect("parse scraped");

    // Combine synthetic + scraped as "candidates"
    let candidates: Vec<&Recipe> = synthetic.iter().chain(scraped.iter()).collect();

    let templates: Vec<(&str, fn(&Recipe) -> String)> = vec![
        ("natural", summary_natural),
        ("title-heavy", summary_title_heavy),
        ("mech-norm", summary_mech_normalized),
        ("llm-norm", summary_llm_normalized),
    ];

    for (template_name, template_fn) in &templates {
        println!("\n{}", "=".repeat(60));
        println!("Template: {template_name}");
        println!("{}", "=".repeat(60));

        // Embed all real recipes
        let real_embeddings: Vec<(&Recipe, Vec<f32>)> = real
            .iter()
            .map(|r| {
                let summary = template_fn(r);
                let emb = embedder.embed(&summary);
                (r, emb)
            })
            .collect();

        // Embed all candidate recipes (synthetic + scraped)
        let syn_embeddings: Vec<(&Recipe, Vec<f32>)> = candidates
            .iter()
            .map(|r| {
                let summary = template_fn(r);
                let emb = embedder.embed(&summary);
                (*r, emb)
            })
            .collect();

        // --- Report 1: Top matches for each candidate recipe ---
        println!("\n--- Candidates → Real matches (top 3) ---");

        let mut table = Table::new();
        table.set_header(vec![
            "Synthetic",
            "Expected",
            "#1 Match",
            "Score",
            "#2 Match",
            "Score",
            "#3 Match",
            "Score",
        ]);

        for (syn_r, syn_emb) in &syn_embeddings {
            let mut scores: Vec<(&Recipe, f32)> = real_embeddings
                .iter()
                .map(|(r, e)| (*r, cosine(syn_emb, e)))
                .collect();
            scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

            let expected = syn_r
                .note
                .as_deref()
                .unwrap_or("")
                .replace("EXPECTED ", "");

            let row: Vec<Cell> = vec![
                Cell::new(truncate(&syn_r.title, 30)),
                Cell::new(truncate(&expected, 20)),
                Cell::new(truncate(&scores[0].0.title, 25)),
                score_cell(scores[0].1),
                Cell::new(truncate(&scores[1].0.title, 25)),
                score_cell(scores[1].1),
                Cell::new(truncate(&scores[2].0.title, 25)),
                score_cell(scores[2].1),
            ];
            table.add_row(row);
        }
        println!("{table}");

        // --- Report 2: Internal real-recipe similarity (interesting pairs) ---
        println!("\n--- Real × Real: pairs with cosine > 0.70 ---");

        let mut pairs: Vec<(usize, usize, f32)> = Vec::new();
        for i in 0..real_embeddings.len() {
            for j in (i + 1)..real_embeddings.len() {
                let sim = cosine(&real_embeddings[i].1, &real_embeddings[j].1);
                if sim > 0.70 {
                    pairs.push((i, j, sim));
                }
            }
        }
        pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

        let mut pair_table = Table::new();
        pair_table.set_header(vec!["Recipe A", "Recipe B", "Cosine"]);
        for (i, j, sim) in &pairs {
            pair_table.add_row(vec![
                Cell::new(truncate(&real_embeddings[*i].0.title, 35)),
                Cell::new(truncate(&real_embeddings[*j].0.title, 35)),
                score_cell(*sim),
            ]);
        }
        if pairs.is_empty() {
            println!("  (no pairs above 0.70)");
        } else {
            println!("{pair_table}");
        }

        // --- Debug: compare specific suspicious pairs ---
        {
            println!("\n--- DEBUG: Suspicious pair analysis ---\n");

            // Key pairs: false-positive (should be low) vs true-positive (should be high)
            let debug_pairs: Vec<(&str, &str)> = vec![
                // FALSE POSITIVES (different dish, should be LOW)
                ("Pečená pražma s medem a sezamem", "Jednoduché kuře na paprice s rýží"),
                ("Kuře ve sladkokyselé omáčce", "Kuře na paprice podle Pauluse"),
                // TRUE POSITIVES (same/similar dish, should be HIGH)
                ("Krémové těstoviny s kuřecím masem a parmazánem", "Kuřecí kousky s těstovinami a smetanovo-sýrovou omáčkou"),
                ("Grilovaný hermelín zabalený v šunce s pikantními fazolemi", "Grilovaný hermelín v troubě"),
                ("Krémová květáková polévka", "Dýňová polévka na sladko"),
                ("Pečené plněné brambory", "Francouzské brambory"),
            ];

            let all_recipes: Vec<(&Recipe, Vec<f32>)> = real_embeddings
                .iter()
                .chain(syn_embeddings.iter())
                .map(|(r, e)| (*r, e.clone()))
                .collect();

            for (name_a, name_b) in &debug_pairs {
                let a = all_recipes.iter().find(|(r, _)| r.title == *name_a);
                let b = all_recipes.iter().find(|(r, _)| r.title == *name_b);
                if let (Some((ra, ea)), Some((rb, eb))) = (a, b) {
                    let sim = cosine(ea, eb);
                    let label = if name_a.contains("pražma") || name_a.contains("sladkokyselé") {
                        "FALSE-POS"
                    } else {
                        "TRUE-POS "
                    };
                    println!("  [{sim:.3}] {label} | {} ↔ {}", truncate(name_a, 40), truncate(name_b, 40));
                }
            }
        }

        // --- Report 3: Distribution stats ---
        let mut all_sims: Vec<f32> = Vec::new();
        for i in 0..real_embeddings.len() {
            for j in (i + 1)..real_embeddings.len() {
                all_sims.push(cosine(&real_embeddings[i].1, &real_embeddings[j].1));
            }
        }
        all_sims.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = all_sims.len();
        println!(
            "\n  Distribution (real×real): min={:.3} p25={:.3} median={:.3} p75={:.3} max={:.3} (n={n})",
            all_sims[0],
            all_sims[n / 4],
            all_sims[n / 2],
            all_sims[3 * n / 4],
            all_sims[n - 1],
        );
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{truncated}…")
    }
}

fn score_cell(score: f32) -> Cell {
    let color = if score >= 0.85 {
        Color::Red
    } else if score >= 0.70 {
        Color::Yellow
    } else {
        Color::Green
    };
    Cell::new(format!("{score:.3}")).fg(color)
}

fn dirs() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
}
