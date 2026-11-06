//! Index lexical BM25 sur les chunks — reconstruit en mémoire depuis sled,
//! comme HNSW. Paramètres de référence : k1 = 1.2, b = 0.75.

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

/// Entrée d'un document dans l'index avec son ID stable.
#[derive(Debug, Clone)]
struct IndexedDoc {
    doc: Bm25Doc,
    /// Longueur en tokens du document.
    len: u32,
    /// Si le document est valide (non supprimé).
    alive: bool,
}

/// Index BM25 en mémoire avec IDs stables pour mises à jour incrémentales.
///
/// `postings` associe un terme à la liste `(doc_id stable, fréquence)`.
/// Les documents sont stockés dans un vec avec des IDs stables (leur index d'insertion),
/// et un drapeau `alive` marque les documents supprimés.
#[derive(Debug, Default)]
pub struct Bm25Index {
    docs: Vec<IndexedDoc>,
    postings: HashMap<String, Vec<(usize, u32)>>,
    /// Somme des longueurs des documents vivants.
    total_len: u64,
    /// Nombre de documents vivants.
    alive_count: usize,
    /// Mapping file_path -> liste des doc_ids (indices dans `docs`).
    file_to_docs: HashMap<String, Vec<usize>>,
}

impl Bm25Index {
    /// Construit l'index à partir des chunks. Coût linéaire en texte indexé.
    pub fn from_docs(docs: Vec<Bm25Doc>) -> Self {
        let mut index = Self::default();
        for doc in docs {
            index.add_doc(doc);
        }
        index
    }

    /// Ajoute un document à l'index.
    fn add_doc(&mut self, doc: Bm25Doc) {
        let doc_id = self.docs.len();
        let tokens = tokenize(&doc.text);
        let len = tokens.len() as u32;

        let mut tf: HashMap<&str, u32> = HashMap::new();
        for token in &tokens {
            *tf.entry(token.as_str()).or_insert(0) += 1;
        }
        for (term, count) in tf {
            self.postings
                .entry(term.to_string())
                .or_default()
                .push((doc_id, count));
        }

        self.total_len += u64::from(len);
        self.alive_count += 1;
        self.file_to_docs
            .entry(doc.file_path.clone())
            .or_default()
            .push(doc_id);

        self.docs.push(IndexedDoc {
            doc,
            len,
            alive: true,
        });
    }

    /// Retire tous les documents d'un fichier de l'index.
    pub fn remove_file(&mut self, file_path: &str) {
        let Some(doc_ids) = self.file_to_docs.remove(file_path) else {
            return;
        };

        for &doc_id in &doc_ids {
            if doc_id >= self.docs.len() {
                continue;
            }
            let indexed = &mut self.docs[doc_id];
            if !indexed.alive {
                continue;
            }
            indexed.alive = false;
            self.total_len -= u64::from(indexed.len);
            self.alive_count = self.alive_count.saturating_sub(1);

            // Retirer de postings (marquer comme mort en filtrant plus tard)
            // Pour l'instant, on laisse les entrées dans postings et on les filtre à la recherche.
            // Une reconstruction périodique nettoiera les entrées mortes.
        }
    }

    /// Remplace tous les documents d'un fichier (remove + add).
    pub fn replace_file(&mut self, file_path: &str, new_docs: Vec<Bm25Doc>) {
        self.remove_file(file_path);
        for doc in new_docs {
            self.add_doc(doc);
        }
    }

    /// Nombre de chunks indexés (vivants).
    pub fn len(&self) -> usize {
        self.alive_count
    }

    /// Vrai quand aucun chunk n'est indexé.
    pub fn is_empty(&self) -> bool {
        self.alive_count == 0
    }

    /// Chunk à l'indice donné (indices rendus par [`Bm25Index::search`]).
    pub fn doc(&self, idx: usize) -> &Bm25Doc {
        &self.docs[idx].doc
    }

    /// Longueur moyenne des documents vivants.
    fn avgdl(&self) -> f64 {
        if self.alive_count == 0 {
            0.0
        } else {
            self.total_len as f64 / self.alive_count as f64
        }
    }

    /// Recherche BM25. Les termes de requête répétés ne comptent qu'une fois
    /// (la fréquence de requête n'apporte rien ici). Rend les `k` meilleurs
    /// `(doc_id, score)` triés par score décroissant.
    pub fn search(&self, query_tokens: &[String], k: usize) -> Vec<(usize, f64)> {
        if self.alive_count == 0 || k == 0 || query_tokens.is_empty() {
            return Vec::new();
        }

        let n = self.alive_count as f64;
        let avgdl = self.avgdl();
        let mut seen: HashMap<&str, ()> = HashMap::new();
        let mut scores: HashMap<usize, f64> = HashMap::new();

        for token in query_tokens {
            if seen.insert(token.as_str(), ()).is_some() {
                continue;
            }
            let Some(posting) = self.postings.get(token.as_str()) else {
                continue;
            };
            let df = posting.len() as f64;
            let idf = (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
            for (doc_idx, tf) in posting {
                // Ignorer les documents morts
                if *doc_idx >= self.docs.len() || !self.docs[*doc_idx].alive {
                    continue;
                }
                let dl = f64::from(self.docs[*doc_idx].len);
                let denom =
                    f64::from(*tf) + K1 * (1.0 - B + B * dl / avgdl.max(f64::MIN_POSITIVE));
                let contribution = idf * (f64::from(*tf) * (K1 + 1.0)) / denom;
                *scores.entry(*doc_idx).or_insert(0.0) += contribution;
            }
        }

        let mut ranked: Vec<(usize, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        ranked.truncate(k);
        ranked
    }

    /// Reconstruit l'index pour nettoyer les entrées mortes (compaction).
    /// À appeler périodiquement quand beaucoup de documents ont été supprimés.
    pub fn compact(&mut self) {
        // Reconstruction complète : recréer un nouvel index avec seulement les docs vivants
        let alive_docs: Vec<Bm25Doc> = self
            .docs
            .iter()
            .filter(|d| d.alive)
            .map(|d| d.doc.clone())
            .collect();
        *self = Self::from_docs(alive_docs);
    }

    /// Nombre de documents morts (pour décider quand compacter).
    pub fn dead_count(&self) -> usize {
        self.docs.len() - self.alive_count
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
