//! Fusion de listes classées (Reciprocal Rank Fusion) et normalisation.
//!
//! `RRF_K = 60` et la normalisation par le maximum reproduisent Socratus.
//! La fusion se fait sur les identifiants de chunk, pas sur les fichiers :
//! deux listes peuvent proposer des chunks différents du même fichier.

use std::cmp::Ordering;
use std::collections::HashMap;

/// Constante de lissage RRF.
pub const RRF_K: u32 = 60;

/// Fusionne des listes classées pondérées.
///
/// Chaque entrée est `(identifiants classés du meilleur au moins bon, poids)`.
/// Score d'un identifiant : `Σ poids / (RRF_K + rang)`, rang commençant à 1.
/// Le résultat est trié par score décroissant ; les identifiants ex æquo
/// gardent un ordre stable (première apparition).
pub fn rrf_fuse(lists: &[(Vec<String>, f64)]) -> Vec<(String, f64)> {
    let mut scores: HashMap<String, f64> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for (ids, weight) in lists {
        for (rank0, id) in ids.iter().enumerate() {
            let contribution = weight / (f64::from(RRF_K) + rank0 as f64 + 1.0);
            match scores.get_mut(id) {
                Some(score) => *score += contribution,
                None => {
                    scores.insert(id.clone(), contribution);
                    order.push(id.clone());
                }
            }
        }
    }

    let mut ranked: Vec<(String, f64)> = order
        .into_iter()
        .map(|id| {
            let score = scores.get(&id).copied().unwrap_or(0.0);
            (id, score)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    ranked
}

/// Normalise des scores par le maximum : le meilleur vaut 1.0, les autres
/// sont ramenés dans `[0, 1]`. Un maximum nul donne une liste de zéros.
pub fn normalize_max(scores: &[f64]) -> Vec<f64> {
    let top = scores.iter().copied().fold(0.0f64, f64::max);
    if top <= 0.0 {
        return vec![0.0; scores.len()];
    }
    scores
        .iter()
        .map(|score| (score / top).clamp(0.0, 1.0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_in_both_lists_wins() {
        let fused = rrf_fuse(&[
            (vec!["shared".into(), "dense-only".into()], 1.0),
            (vec!["lexical-only".into(), "shared".into()], 1.0),
        ]);
        assert_eq!(fused[0].0, "shared");
        assert!(fused[0].1 > fused[1].1);
    }

    #[test]
    fn weights_shift_the_ranking() {
        let fused = rrf_fuse(&[(vec!["dense".into()], 1.0), (vec!["lexical".into()], 2.0)]);
        assert_eq!(fused[0].0, "lexical");
    }

    #[test]
    fn rank_decays_the_score() {
        let fused = rrf_fuse(&[(vec!["first".into(), "second".into()], 1.0)]);
        assert_eq!(fused[0].0, "first");
        assert!(fused[0].1 > fused[1].1);
    }

    #[test]
    fn normalize_maps_top_to_one_and_zero_stays_zero() {
        assert_eq!(normalize_max(&[4.0, 2.0, 0.0]), vec![1.0, 0.5, 0.0]);
        assert_eq!(normalize_max(&[0.0, 0.0]), vec![0.0, 0.0]);
        assert!(normalize_max(&[]).is_empty());
    }

    #[test]
    fn empty_lists_fuse_to_nothing() {
        assert!(rrf_fuse(&[]).is_empty());
        assert!(rrf_fuse(&[(Vec::new(), 1.0)]).is_empty());
    }
}
