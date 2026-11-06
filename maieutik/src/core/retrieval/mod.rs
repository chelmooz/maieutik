//! Recherche hybride — lexicale (BM25) + dense (HNSW), fusion RRF.
//!
//! Ce module ne dépend ni d'ONNX ni de sled : il manipule des scores et des
//! textes. `VectorIndex` fournit les listes classées, la fusion et la
//! sélection se font ici.

pub mod bm25;
pub mod fusion;
pub mod passage;
pub mod query;

pub use bm25::{Bm25Doc, Bm25Index};
pub use fusion::{normalize_max, rrf_fuse};
pub use passage::{select_passages, Candidate};
pub use query::{analyze, tokenize, QueryAnalysis};

/// Mode de recherche demandé par l'appelant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// Vecteurs uniquement (comportement historique).
    Semantic,
    /// BM25 uniquement.
    Lexical,
    /// Les deux listes fusionnées par RRF, puis sélection de passages.
    Hybrid,
}

impl std::fmt::Display for SearchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            SearchMode::Semantic => "semantic",
            SearchMode::Lexical => "lexical",
            SearchMode::Hybrid => "hybrid",
        };
        f.write_str(name)
    }
}

impl std::str::FromStr for SearchMode {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.to_ascii_lowercase().as_str() {
            "semantic" | "dense" => Ok(SearchMode::Semantic),
            "lexical" | "bm25" | "keyword" => Ok(SearchMode::Lexical),
            "hybrid" => Ok(SearchMode::Hybrid),
            other => Err(format!(
                "mode inconnu : {other} (attendu : semantic, lexical ou hybrid)"
            )),
        }
    }
}

/// Résultat de recherche avec provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub file_path: String,
    pub text: String,
    pub heading_path: Option<String>,
    pub chunk_index: usize,
    /// Similarité normalisée `[0, 1]` (1 = meilleur). Les scores de modes
    /// différents ne sont comparables qu'à l'intérieur d'un même mode.
    pub score: f32,
    /// Mode qui a produit ce hit.
    pub source: SearchMode,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parses_aliases() {
        assert_eq!(
            "semantic".parse::<SearchMode>().expect("test"),
            SearchMode::Semantic
        );
        assert_eq!("dense".parse::<SearchMode>().expect("test"), SearchMode::Semantic);
        assert_eq!("bm25".parse::<SearchMode>().expect("test"), SearchMode::Lexical);
        assert_eq!("HYBRID".parse::<SearchMode>().expect("test"), SearchMode::Hybrid);
        assert!("fuzzy".parse::<SearchMode>().is_err());
    }
}
