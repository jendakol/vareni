//! In-process ONNX embedding using all-MiniLM-L6-v2 (384 dimensions).
//! Adapted from second-brain crate — same model, same normalization.

use std::path::Path;
use std::sync::Mutex;

use ndarray::{Array2, Axis};
use ort::session::Session;
use ort::value::Tensor;

/// Stop-ingredients that carry no semantic signal for dedup.
const STOP_INGREDIENTS: &[&str] = &[
    "sůl",
    "pepř",
    "sůl a pepř",
    "pepř a sůl",
    "olej",
    "olivový olej",
    "rostlinný olej",
    "česnek",
    "cibule",
    "voda",
    "máslo",
    "smetana",
    "černý pepř",
    "bílý pepř",
    "mletý pepř",
    "řepkový olej",
    "neutrální olej",
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
        let encoding = match self.tokenizer.encode(text, true) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(
                    "Tokenization failed for text '{}...': {e}",
                    &text[..text.len().min(50)]
                );
                return None;
            }
        };
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
        let token_type_ids = Array2::from_shape_vec(
            (1, seq_len),
            type_ids.iter().map(|&x| i64::from(x)).collect(),
        )
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
    pub fn recipe_summary(canonical_name: &str, tags: &[String], ingredients: &[String]) -> String {
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

    /// Build a quick mechanical embedding text (no canonical name yet -- used for pre-filtering).
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

#[cfg(test)]
mod tests {
    use super::*;

    // -- is_stop_ingredient --

    #[test]
    fn stop_ingredient_sul() {
        assert!(is_stop_ingredient("sůl"));
    }

    #[test]
    fn stop_ingredient_olivovy_olej() {
        assert!(is_stop_ingredient("olivový olej"));
    }

    #[test]
    fn non_stop_ingredient_kureci_prsa() {
        assert!(!is_stop_ingredient("kuřecí prsa"));
    }

    #[test]
    fn stop_ingredient_case_insensitive() {
        assert!(is_stop_ingredient("Sůl"));
    }

    #[test]
    fn stop_ingredient_cibule() {
        assert!(is_stop_ingredient("cibule"));
    }

    #[test]
    fn stop_ingredient_maslo() {
        assert!(is_stop_ingredient("máslo"));
    }

    #[test]
    fn non_stop_ingredient_paprika() {
        assert!(!is_stop_ingredient("paprika"));
    }

    // -- recipe_summary --

    #[test]
    fn recipe_summary_structure_and_filtering() {
        let tags = vec!["kuřecí".to_string(), "česká kuchyně".to_string()];
        let ingredients = vec![
            "kuřecí prsa".to_string(),
            "sůl".to_string(),
            "pepř".to_string(),
            "cibule".to_string(),
            "paprika".to_string(),
            "smetana".to_string(),
            "mouka".to_string(),
        ];

        let summary = EmbeddingService::recipe_summary("kuře na paprice", &tags, &ingredients);

        // Starts with repeated canonical name
        assert!(
            summary.starts_with("kuře na paprice. kuře na paprice."),
            "expected repeated canonical name prefix, got: {summary}"
        );

        // Contains tags
        assert!(
            summary.contains("Kategorie: kuřecí, česká kuchyně"),
            "expected tags in summary, got: {summary}"
        );

        // Contains non-stop ingredients
        assert!(
            summary.contains("kuřecí prsa"),
            "expected 'kuřecí prsa' in: {summary}"
        );
        assert!(
            summary.contains("paprika"),
            "expected 'paprika' in: {summary}"
        );

        // Does NOT contain stop ingredients
        assert!(!summary.contains("sůl"), "unexpected 'sůl' in: {summary}");
        assert!(!summary.contains("pepř"), "unexpected 'pepř' in: {summary}");
        assert!(
            !summary.contains("cibule"),
            "unexpected 'cibule' in: {summary}"
        );

        // At most 5 ingredients in the Obsahuje section
        if let Some(obsahuje) = summary.split("Obsahuje: ").nth(1) {
            let ing_section = obsahuje.trim_end_matches('.');
            let count = ing_section.split(", ").count();
            assert!(
                count <= 5,
                "expected at most 5 ingredients, got {count}: {obsahuje}"
            );
        }
    }

    #[test]
    fn recipe_summary_empty_tags() {
        let summary = EmbeddingService::recipe_summary("test", &[], &["kuřecí prsa".to_string()]);
        assert!(!summary.contains("Kategorie"));
        assert!(summary.contains("kuřecí prsa"));
    }

    #[test]
    fn recipe_summary_all_stop_ingredients() {
        let ingredients = vec!["sůl".to_string(), "pepř".to_string(), "voda".to_string()];
        let summary = EmbeddingService::recipe_summary("test", &[], &ingredients);
        assert!(!summary.contains("Obsahuje"));
    }

    // -- cosine_similarity --

    #[test]
    fn cosine_identical_vectors() {
        let v = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }
}
