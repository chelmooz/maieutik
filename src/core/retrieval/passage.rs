//! Sélection de passages — dédoublonnage, planchers, plafond par fichier et
//! budget. Constantes reprises de Socratus.

use super::query::tokenize;
use std::cmp::Ordering;
use std::collections::HashSet;

/// Deux passages dont les ensembles de termes se recouvrent à ce point sont
/// considérés comme des quasi-doublons ; le moins bien classé est écarté.
pub const NEAR_DUPLICATE_SIMILARITY: f64 = 0.92;
/// Un candidat dense sous cette similarité cosinus est écarté.
pub const DENSE_FLOOR_SIMILARITY: f32 = 0.25;
/// Nombre minimal de termes pour qu'un passage soit coupé proprement.
pub const MIN_CUT_TOKENS: usize = 30;
/// Budget par défaut, en tokens approximés (`caractères / 4`).
pub const DEFAULT_MAX_TOKENS: usize = 1200;
/// Nombre maximal de passages retenus par fichier.
pub const DEFAULT_PER_FILE_LIMIT: usize = 2;

/// Candidat issu de la fusion avant sélection.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: String,
    pub file_path: String,
    pub text: String,
    pub heading_path: Option<String>,
    pub chunk_index: usize,
    /// Score fusionné normalisé `[0, 1]`.
    pub score: f32,
    /// Similarité dense quand le candidat vient de la liste sémantique.
    pub dense_similarity: Option<f32>,
    /// Rang (1-based) dans la liste lexicale quand il en vient.
    pub lexical_rank: Option<usize>,
}

/// Approxime un nombre de tokens sans charger de tokenizer : quatre
/// caractères par token. Le budget reste une garde, pas une mesure exacte.
pub fn approx_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// Similarité de Jaccard entre deux ensembles de termes.
fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let intersection = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

/// Sélectionne les passages à rendre au client.
///
/// Ordre des filtres : plancher dense → tri → quasi-doublons → plafond par
/// fichier → budget. Un candidat lexical n'a pas de similarité dense : il
/// n'est jamais écarté par le plancher (BM25 a déjà classé).
pub fn select_passages(
    mut candidates: Vec<Candidate>,
    max_tokens: usize,
    per_file_limit: usize,
) -> Vec<Candidate> {
    // 1. Plancher dense.
    candidates.retain(|c| match c.dense_similarity {
        Some(similarity) => similarity >= DENSE_FLOOR_SIMILARITY,
        None => true,
    });

    // 2. Meilleur score d'abord.
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

    // 3. Quasi-doublons, puis plafond par fichier.
    let mut kept: Vec<Candidate> = Vec::new();
    let mut token_sets: Vec<HashSet<String>> = Vec::new();
    let mut per_file: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for candidate in candidates {
        let tokens: HashSet<String> = tokenize(&candidate.text).into_iter().collect();
        let duplicate = token_sets
            .iter()
            .any(|kept_tokens| jaccard(kept_tokens, &tokens) >= NEAR_DUPLICATE_SIMILARITY);
        if duplicate {
            continue;
        }
        let count = per_file.entry(candidate.file_path.clone()).or_insert(0);
        if *count >= per_file_limit {
            continue;
        }
        *count += 1;
        token_sets.push(tokens);
        kept.push(candidate);
    }

    // 4. Budget : on garde au moins un passage, jamais un dépassement silencieux.
    let mut total_tokens = 0usize;
    let mut selected = Vec::new();
    for candidate in kept {
        let cost = approx_tokens(&candidate.text);
        if !selected.is_empty() && total_tokens + cost > max_tokens {
            continue;
        }
        total_tokens += cost;
        selected.push(candidate);
    }

    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, file: &str, text: &str, score: f32) -> Candidate {
        Candidate {
            id: id.to_string(),
            file_path: file.to_string(),
            text: text.to_string(),
            heading_path: None,
            chunk_index: 0,
            score,
            dense_similarity: Some(0.9),
            lexical_rank: None,
        }
    }

    #[test]
    fn dense_floor_drops_weak_candidates() {
        let mut weak = candidate("a#0", "a.md", "rust compiler", 0.9);
        weak.dense_similarity = Some(0.1);
        let kept = select_passages(vec![weak], 1000, 2);
        assert!(kept.is_empty());
    }

    #[test]
    fn lexical_candidate_survives_without_dense_similarity() {
        let mut lexical = candidate("a#0", "a.md", "rust compiler", 0.9);
        lexical.dense_similarity = None;
        lexical.lexical_rank = Some(1);
        assert_eq!(select_passages(vec![lexical], 1000, 2).len(), 1);
    }

    #[test]
    fn near_duplicates_are_collapsed() {
        let text = "the rust borrow checker rejects this program";
        let kept = select_passages(
            vec![
                candidate("a#0", "a.md", text, 0.9),
                candidate("a#1", "a.md", text, 0.8),
            ],
            1000,
            5,
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "a#0");
    }

    #[test]
    fn per_file_limit_caps_results() {
        let kept = select_passages(
            vec![
                candidate("a#0", "a.md", "alpha beta gamma delta", 0.9),
                candidate("a#1", "a.md", "epsilon zeta eta theta", 0.8),
                candidate("a#2", "a.md", "iota kappa lambda mu", 0.7),
            ],
            1000,
            2,
        );
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn budget_stops_before_overflow_but_keeps_one() {
        let long = "mot ".repeat(1000); // ~1001 tokens approx
        let kept = select_passages(vec![candidate("a#0", "a.md", &long, 0.9)], 10, 2);
        assert_eq!(kept.len(), 1, "le premier passage est toujours gardé");
    }

    #[test]
    fn results_stay_sorted_by_score() {
        let kept = select_passages(
            vec![
                candidate("a#0", "a.md", "alpha beta", 0.5),
                candidate("b#0", "b.md", "gamma delta", 0.9),
            ],
            1000,
            2,
        );
        assert_eq!(kept[0].id, "b#0");
    }
}
