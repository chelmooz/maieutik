//! Sélection de passages — dédoublonnage, planchers, plafond par fichier et
//! budget. Constantes de référence.

use super::query::tokenize;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

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
/// Rang minimal BM25 (1-based) pour qu'un candidat lexical soit éligible.
/// Constante de référence : `Bm25FloorRank = 10`.
pub const BM25_FLOOR_RANK: usize = 10;
/// Si vrai, essaie de remplacer un chunk par le fichier entier quand le
/// fichier tient dans le budget et améliore le rappel.
pub const TRY_WHOLE_FILE: bool = true;
/// Si vrai, fusionne les chunks adjacents du même fichier en un seul passage.
pub const MERGE_ADJACENT: bool = true;

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

/// Charge le contenu complet d'un fichier si possible.
fn load_whole_file(file_path: &str) -> Option<String> {
    std::fs::read_to_string(file_path).ok()
}

/// Fusionne des chunks adjacents du même fichier.
fn merge_adjacent_chunks(candidates: &[Candidate]) -> Vec<Candidate> {
    if !MERGE_ADJACENT {
        return candidates.to_vec();
    }

    // Grouper par fichier
    let mut by_file: HashMap<String, Vec<&Candidate>> = HashMap::new();
    for c in candidates {
        by_file.entry(c.file_path.clone()).or_default().push(c);
    }

    let mut merged = Vec::new();
    for (file_path, mut file_candidates) in by_file {
        // Trier par chunk_index
        file_candidates.sort_by_key(|c| c.chunk_index);

        let mut current_group: Vec<&Candidate> = Vec::new();
        for c in file_candidates {
            if let Some(last) = current_group.last() {
                if c.chunk_index == last.chunk_index + 1 {
                    current_group.push(c);
                } else {
                    // Finaliser le groupe précédent
                    if current_group.len() > 1 {
                        merged.push(merge_group(&current_group, &file_path));
                    } else {
                        merged.push((*current_group[0]).clone());
                    }
                    current_group.clear();
                    current_group.push(c);
                }
            } else {
                current_group.push(c);
            }
        }
        // Dernier groupe
        if current_group.len() > 1 {
            merged.push(merge_group(&current_group, &file_path));
        } else {
            merged.push((*current_group[0]).clone());
        }
    }

    merged
}

fn merge_group(group: &[&Candidate], file_path: &str) -> Candidate {
    let first = group[0];
    let mut text = String::new();
    for (i, c) in group.iter().enumerate() {
        if i > 0 {
            text.push(' ');
        }
        text.push_str(&c.text);
    }
    Candidate {
        id: format!("{}#merged-{}", file_path, first.chunk_index),
        file_path: file_path.to_string(),
        text,
        heading_path: first.heading_path.clone(),
        chunk_index: first.chunk_index,
        score: group.iter().map(|c| c.score).max_by(f32::total_cmp).unwrap_or(0.0),
        dense_similarity: group.iter().filter_map(|c| c.dense_similarity).max_by(f32::total_cmp),
        lexical_rank: group.iter().filter_map(|c| c.lexical_rank).min(),
    }
}

/// Essaie de remplacer un chunk par le fichier entier si le budget le permet.
fn try_whole_file(
    candidate: &Candidate,
    max_tokens: usize,
    total_tokens: usize,
) -> Option<Candidate> {
    if !TRY_WHOLE_FILE {
        return None;
    }
    let whole = load_whole_file(&candidate.file_path)?;
    let whole_tokens = approx_tokens(&whole);
    // Ne remplace que si le fichier entier tient dans le budget restant
    if total_tokens + whole_tokens > max_tokens {
        return None;
    }
    Some(Candidate {
        id: format!("{}#whole", candidate.file_path),
        file_path: candidate.file_path.clone(),
        text: whole,
        heading_path: candidate.heading_path.clone(),
        chunk_index: 0,
        score: candidate.score,
        dense_similarity: candidate.dense_similarity,
        lexical_rank: candidate.lexical_rank,
    })
}

/// Sélectionne les passages à rendre au client.
///
/// Ordre des filtres : plancher dense → plancher BM25 → tri → quasi-doublons
/// → TryWholeFile → MergeAdjacent → plafond par fichier → budget.
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

    // 2. Plancher BM25 : écarter les candidats lexicaux au-delà du rang 10.
    candidates.retain(|c| match c.lexical_rank {
        Some(rank) => rank <= BM25_FLOOR_RANK,
        None => true,
    });

    // 3. Meilleur score d'abord.
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

    // 4. Quasi-doublons.
    let mut kept: Vec<Candidate> = Vec::new();
    let mut token_sets: Vec<HashSet<String>> = Vec::new();

    for candidate in candidates {
        let tokens: HashSet<String> = tokenize(&candidate.text).into_iter().collect();
        let duplicate = token_sets
            .iter()
            .any(|kept_tokens| jaccard(kept_tokens, &tokens) >= NEAR_DUPLICATE_SIMILARITY);
        if duplicate {
            continue;
        }
        token_sets.push(tokens);
        kept.push(candidate);
    }

    // 5. TryWholeFile : pour chaque candidat gardé, tenter de le remplacer par le fichier entier.
    let mut with_whole = Vec::new();
    let mut total_tokens = 0usize;
    for candidate in kept {
        let cost = approx_tokens(&candidate.text);
        if let Some(whole) = try_whole_file(&candidate, max_tokens, total_tokens) {
            total_tokens += approx_tokens(&whole.text);
            with_whole.push(whole);
        } else {
            total_tokens += cost;
            with_whole.push(candidate);
        }
    }

    // 6. MergeAdjacent : fusionner les chunks adjacents du même fichier.
    let merged = merge_adjacent_chunks(&with_whole);

    // 7. Plafond par fichier.
    let mut per_file: HashMap<String, usize> = HashMap::new();
    let mut after_per_file = Vec::new();
    for candidate in merged {
        let count = per_file.entry(candidate.file_path.clone()).or_insert(0);
        if *count >= per_file_limit {
            continue;
        }
        *count += 1;
        after_per_file.push(candidate);
    }

    // 8. Budget final : on garde au moins un passage, jamais un dépassement silencieux.
    let mut final_total = 0usize;
    let mut selected = Vec::new();
    for candidate in after_per_file {
        let cost = approx_tokens(&candidate.text);
        if !selected.is_empty() && final_total + cost > max_tokens {
            continue;
        }
        final_total += cost;
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
        // Utiliser des chemins qui n'existent pas pour éviter TryWholeFile
        let c1 = candidate("a#0", "/nonexistent/results_stay_sorted_by_score_a.md", "alpha beta", 0.5);
        let c2 = candidate("b#0", "/nonexistent/results_stay_sorted_by_score_b.md", "gamma delta", 0.9);
        let kept = select_passages(vec![c1, c2], 1000, 2);
        assert_eq!(kept[0].id, "b#0");
    }

    #[test]
    fn bm25_floor_rank_drops_low_ranked() {
        let mut low = candidate("a#0", "/nonexistent/a.md", "rust compiler", 0.9);
        low.dense_similarity = None;
        low.lexical_rank = Some(15);
        let kept = select_passages(vec![low], 1000, 2);
        assert!(kept.is_empty(), "rang BM25 15 doit être écarté");

        let mut ok = candidate("b#0", "/nonexistent/b.md", "rust compiler", 0.9);
        ok.dense_similarity = None;
        ok.lexical_rank = Some(5);
        let kept = select_passages(vec![ok], 1000, 2);
        assert_eq!(kept.len(), 1, "rang BM25 5 doit être gardé");
    }

    #[test]
    fn merge_adjacent_chunks_combines_neighbors() {
        let c1 = Candidate {
            id: "a#0".into(),
            file_path: "/nonexistent/a.md".into(),
            text: "premier chunk".into(),
            heading_path: None,
            chunk_index: 0,
            score: 0.9,
            dense_similarity: Some(0.9),
            lexical_rank: None,
        };
        let c2 = Candidate {
            id: "a#1".into(),
            file_path: "/nonexistent/a.md".into(),
            text: "deuxième chunk".into(),
            heading_path: None,
            chunk_index: 1,
            score: 0.8,
            dense_similarity: Some(0.8),
            lexical_rank: None,
        };
        let c3 = Candidate {
            id: "a#3".into(),
            file_path: "/nonexistent/a.md".into(),
            text: "quatrième chunk (écart)".into(),
            heading_path: None,
            chunk_index: 3,
            score: 0.7,
            dense_similarity: Some(0.7),
            lexical_rank: None,
        };
        let kept = select_passages(vec![c1, c2, c3], 1000, 5);
        assert_eq!(kept.len(), 2);
        let merged = kept.iter().find(|c| c.id.contains("merged")).unwrap();
        assert!(merged.text.contains("premier chunk"));
        assert!(merged.text.contains("deuxième chunk"));
        assert!(!merged.text.contains("quatrième"));
    }

    #[test]
    fn try_whole_file_replaces_chunk_when_fits() {
        // Créer un fichier temporaire
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let content = "contenu du fichier entier qui tient dans le budget".repeat(10);
        std::fs::write(tmp.path(), &content).unwrap();

        let c = Candidate {
            id: format!("{}#0", tmp.path().display()),
            file_path: tmp.path().to_string_lossy().into_owned(),
            text: "petit chunk".into(),
            heading_path: None,
            chunk_index: 0,
            score: 0.9,
            dense_similarity: Some(0.9),
            lexical_rank: None,
        };
        let kept = select_passages(vec![c], 5000, 2);
        assert_eq!(kept.len(), 1);
        assert!(kept[0].id.contains("#whole"));
        assert!(kept[0].text.len() > 100); // Le fichier entier est plus gros
    }
}
