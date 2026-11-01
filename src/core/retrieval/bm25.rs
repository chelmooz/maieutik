//! Index lexical BM25 sur les chunks — reconstruit en mémoire depuis sled,
//! comme HNSW. Paramètres identiques à Socratus : k1 = 1.2, b = 0.75.

use super::query::tokenize;
use std::cmp::Ordering;
use std::collections::HashMap;

/// Saturation de la fréquence de terme.
pub const K1: f64 = 1.2;
/// Normalisation par la longueur du document.
pub const B: f64 = 0.75;

/// Un chunk indexé lexicalement.
#[derive(Debug, Clone)]
pub struct Bm25Doc {
    /// Identifiant du chunk (`chemin#index`), clé de jointure avec les vecteurs.
    pub id: String,
    pub file_path: String,
    pub text: String,
    pub heading_path: Option<String>,
    pub chunk_index: usize,
}

/// Index BM25 en mémoire.
///
/// `postings` associe un terme à la liste `(indice de document, fréquence)`.
/// Les documents sont stockés à plat : la sélection de passages relit le texte
/// sans retourner dans sled.
#[derive(Debug, Default)]
pub struct Bm25Index {
    docs: Vec<Bm25Doc>,
    postings: HashMap<String, Vec<(usize, u32)>>,
    doc_lens: Vec<u32>,
    avgdl: f64,
}

impl Bm25Index {
    /// Construit l'index à partir des chunks. Coût linéaire en texte indexé.
    pub fn from_docs(docs: Vec<Bm25Doc>) -> Self {
        let mut postings: HashMap<String, Vec<(usize, u32)>> = HashMap::new();
        let mut doc_lens = Vec::with_capacity(docs.len());

        for (doc_idx, doc) in docs.iter().enumerate() {
            let tokens = tokenize(&doc.text);
            doc_lens.push(tokens.len() as u32);
            let mut tf: HashMap<&str, u32> = HashMap::new();
            for token in &tokens {
                *tf.entry(token.as_str()).or_insert(0) += 1;
            }
            for (term, count) in tf {
                postings
                    .entry(term.to_string())
                    .or_default()
                    .push((doc_idx, count));
            }
        }

        let total: u64 = doc_lens.iter().map(|n| u64::from(*n)).sum();
        let avgdl = if doc_lens.is_empty() {
            0.0
        } else {
            total as f64 / doc_lens.len() as f64
        };

        Self {
            docs,
            postings,
            doc_lens,
            avgdl,
        }
    }

    /// Nombre de chunks indexés.
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// Vrai quand aucun chunk n'est indexé.
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Chunk à l'indice donné (indices rendus par [`Bm25Index::search`]).
    pub fn doc(&self, idx: usize) -> &Bm25Doc {
        &self.docs[idx]
    }

    /// Recherche BM25. Les termes de requête répétés ne comptent qu'une fois
    /// (la fréquence de requête n'apporte rien ici). Rend les `k` meilleurs
    /// `(indice de document, score)` triés par score décroissant.
    pub fn search(&self, query_tokens: &[String], k: usize) -> Vec<(usize, f64)> {
        if self.docs.is_empty() || k == 0 || query_tokens.is_empty() {
            return Vec::new();
        }

        let n = self.docs.len() as f64;
        let mut seen: HashMap<&str, ()> = HashMap::new();
        let mut scores: HashMap<usize, f64> = HashMap::new();

        for token in query_tokens {
            // Terme déjà traité dans cette requête.
            if seen.insert(token.as_str(), ()).is_some() {
                continue;
            }
            let Some(posting) = self.postings.get(token.as_str()) else {
                continue;
            };
            let df = posting.len() as f64;
            let idf = (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
            for (doc_idx, tf) in posting {
                let dl = f64::from(self.doc_lens[*doc_idx]);
                let denom =
                    f64::from(*tf) + K1 * (1.0 - B + B * dl / self.avgdl.max(f64::MIN_POSITIVE));
                let contribution = idf * (f64::from(*tf) * (K1 + 1.0)) / denom;
                *scores.entry(*doc_idx).or_insert(0.0) += contribution;
            }
        }

        let mut ranked: Vec<(usize, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        ranked.truncate(k);
        ranked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, text: &str) -> Bm25Doc {
        Bm25Doc {
            id: id.to_string(),
            file_path: id.split('#').next().unwrap_or(id).to_string(),
            text: text.to_string(),
            heading_path: None,
            chunk_index: 0,
        }
    }

    #[test]
    fn rare_term_outranks_common_term() {
        let index = Bm25Index::from_docs(vec![
            doc("a#0", "rust compiler borrow checker"),
            doc("b#0", "rust rust rust everyday notes"),
            doc("c#0", "sourdough bread baking"),
        ]);
        let hits = index.search(&["checker".to_string()], 3);
        assert_eq!(hits.len(), 1);
        assert_eq!(index.doc(hits[0].0).id, "a#0");
    }

    #[test]
    fn more_occurrences_score_higher() {
        let index = Bm25Index::from_docs(vec![
            doc("a#0", "sourdough sourdough sourdough starter"),
            doc("b#0", "sourdough starter"),
        ]);
        let hits = index.search(&["sourdough".to_string()], 2);
        assert_eq!(hits.len(), 2);
        assert_eq!(index.doc(hits[0].0).id, "a#0");
        assert!(hits[0].1 > hits[1].1);
    }

    #[test]
    fn unknown_terms_return_nothing() {
        let index = Bm25Index::from_docs(vec![doc("a#0", "rust compiler")]);
        assert!(index.search(&["kubernetes".to_string()], 5).is_empty());
    }

    #[test]
    fn repeated_query_terms_do_not_double_count() {
        let index = Bm25Index::from_docs(vec![doc("a#0", "rust compiler")]);
        let once = index.search(&["rust".to_string()], 1);
        let twice = index.search(&["rust".to_string(), "rust".to_string()], 1);
        assert_eq!(once, twice);
    }

    #[test]
    fn empty_index_is_empty() {
        let index = Bm25Index::from_docs(Vec::new());
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
        assert!(index.search(&["anything".to_string()], 3).is_empty());
    }
}
