//! Analyse de requête — tokenisation partagée par BM25 et la fusion.
//!
//! Les pondérations reproduisent celles de Socratus (vérifiées dans le
//! binaire livré) : le dense pèse 1.0, le lexical 1.0 par défaut, 1.5 pour une
//! requête courte (≤ 5 mots) et 2.0 quand la requête ressemble à un
//! identifiant de code (`snake_case`, `camelCase`, `::`, `()`).

/// Au-delà de ce nombre de mots, la requête n'est plus considérée comme courte.
pub const SHORT_QUERY_WORD_LIMIT: usize = 5;

/// Poids de la liste dense dans la fusion RRF.
pub const DENSE_WEIGHT: f64 = 1.0;
/// Poids lexical quand la requête contient un identifiant de code.
pub const IDENTIFIER_LEXICAL_WEIGHT: f64 = 2.0;
/// Poids lexical d'une requête courte.
pub const SHORT_QUERY_LEXICAL_WEIGHT: f64 = 1.5;
/// Poids lexical par défaut.
pub const DEFAULT_LEXICAL_WEIGHT: f64 = 1.0;

/// Mots vides FR + EN. Ils ne portent pas de signal lexical : les indexer
/// gonfle les postings et fait remonter des chunks hors sujet.
const STOPWORDS: &[&str] = &[
    // français
    "le", "la", "les", "un", "une", "des", "de", "du", "et", "ou", "au", "aux", "ce", "cette",
    "ces", "il", "elle", "ils", "elles", "on", "nous", "vous", "je", "tu", "est", "sont", "ete",
    "etre", "avoir", "dans", "pour", "par", "sur", "avec", "sans", "que", "qui", "quoi", "dont",
    "son", "sa", "ses", "leur", "leurs", "mon", "ma", "mes", "ton", "ta", "tes", "notre", "votre",
    "plus", "moins", "tres", "tout", "tous", "toute", "toutes", "aussi", "mais", "donc", "car",
    "ne", "pas", "se", "sont", "cet", "cette", "comme", "entre", "vers", "chez", "sous",
    // anglais
    "the", "an", "and", "or", "of", "to", "in", "on", "at", "by", "from", "with", "without", "is",
    "are", "was", "were", "be", "been", "being", "this", "that", "these", "those", "it", "its",
    "as", "for", "not", "but", "so", "than", "then", "there", "here", "when", "where", "which",
    "who", "whom", "whose", "what", "how", "why", "all", "any", "each", "some", "such", "into",
    "over", "under", "about", "after", "before", "between", "through", "during",
];

/// Résultat de l'analyse d'une requête.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryAnalysis {
    /// Termes normalisés, sans mots vides, prêts pour BM25.
    pub tokens: Vec<String>,
    /// Vrai si la requête fait au plus [`SHORT_QUERY_WORD_LIMIT`] mots.
    pub is_short: bool,
    /// Vrai si un mot ressemble à un identifiant de code.
    pub has_identifier: bool,
    /// Poids de la liste dense dans la fusion.
    pub dense_weight: f64,
    /// Poids de la liste lexicale dans la fusion.
    pub lexical_weight: f64,
}

/// Tokenise un texte : minuscules, séparation sur tout caractère non
/// alphanumérique, découpage `camelCase`, puis suppression des mots vides et
/// des jetons d'un seul caractère.
///
/// Le découpage `camelCase` est volontaire : `borrowChecker` et
/// `borrow_checker` produisent les mêmes termes, donc une requête écrite dans
/// l'une des deux formes retrouve un document écrit dans l'autre.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split(|c: char| !c.is_alphanumeric()) {
        if raw.is_empty() {
            continue;
        }
        for piece in split_camel(raw) {
            let token = piece.to_lowercase();
            if token.chars().count() >= 2 && !STOPWORDS.contains(&token.as_str()) {
                out.push(token);
            }
        }
    }
    out
}

/// Découpe `camelCase` / `PascalCase` aux frontières minuscule→majuscule.
fn split_camel(token: &str) -> Vec<String> {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 2 {
        return vec![token.to_string()];
    }
    let mut pieces = Vec::new();
    let mut start = 0;
    for i in 1..chars.len() {
        if chars[i - 1].is_lowercase() && chars[i].is_uppercase() {
            pieces.push(chars[start..i].iter().collect());
            start = i;
        }
    }
    pieces.push(chars[start..].iter().collect());
    pieces
}

/// Vrai si le mot ressemble à un identifiant de code : souligné, `::`,
/// appel `()`, ou majuscule interne (`borrowChecker`).
fn is_identifier(raw: &str) -> bool {
    if raw.contains('_') || raw.contains("::") || raw.ends_with("()") {
        return true;
    }
    let chars: Vec<char> = raw.chars().collect();
    chars.len() > 1 && chars.iter().skip(1).any(|c| c.is_uppercase())
}

/// Analyse une requête : termes, détection de requête courte / identifiant,
/// et pondérations de fusion associées.
pub fn analyze(query: &str) -> QueryAnalysis {
    let tokens = tokenize(query);
    let is_short = tokens.len() <= SHORT_QUERY_WORD_LIMIT;
    let has_identifier = query.split_whitespace().any(is_identifier);
    let lexical_weight = if has_identifier {
        IDENTIFIER_LEXICAL_WEIGHT
    } else if is_short {
        SHORT_QUERY_LEXICAL_WEIGHT
    } else {
        DEFAULT_LEXICAL_WEIGHT
    };
    QueryAnalysis {
        tokens,
        is_short,
        has_identifier,
        dense_weight: DENSE_WEIGHT,
        lexical_weight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_lowercases_and_drops_stopwords() {
        assert_eq!(
            tokenize("The quick brown FOX jumps over the lazy dog"),
            vec!["quick", "brown", "fox", "jumps", "lazy", "dog"]
        );
    }

    #[test]
    fn tokenize_splits_camel_case_and_underscores() {
        assert_eq!(tokenize("borrowChecker"), tokenize("borrow_checker"));
        assert_eq!(tokenize("borrow_checker"), vec!["borrow", "checker"]);
    }

    #[test]
    fn short_query_gets_lexical_boost() {
        let a = analyze("rust borrow");
        assert!(a.is_short);
        assert!(!a.has_identifier);
        assert_eq!(a.lexical_weight, SHORT_QUERY_LEXICAL_WEIGHT);
    }

    #[test]
    fn identifier_gets_stronger_lexical_boost() {
        let a = analyze("fn borrow_checker lifetimes");
        assert!(a.has_identifier);
        assert_eq!(a.lexical_weight, IDENTIFIER_LEXICAL_WEIGHT);
    }

    #[test]
    fn long_query_keeps_neutral_weight() {
        let a = analyze("how does the rust borrow checker work with lifetimes and traits");
        assert!(!a.is_short);
        assert_eq!(a.lexical_weight, DEFAULT_LEXICAL_WEIGHT);
    }

    #[test]
    fn capitalized_word_is_not_an_identifier() {
        let a = analyze("Rust compiler");
        assert!(!a.has_identifier);
    }
}
