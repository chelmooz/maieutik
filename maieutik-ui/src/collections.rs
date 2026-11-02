//! Collections documentaires `sources/`, `wiki/`, `eval/`.
//! `eval/` ne doit JAMAIS apparaître dans les résultats de production.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Collection {
    Sources,
    Wiki,
    Eval,
    Other,
}

impl Collection {
    pub fn of(path: &Path) -> Self {
        for comp in path.components() {
            match comp.as_os_str().to_str() {
                Some("eval") => return Self::Eval,
                Some("wiki") => return Self::Wiki,
                Some("sources") => return Self::Sources,
                _ => {}
            }
        }
        Self::Other
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sources => "sources",
            Self::Wiki => "wiki",
            Self::Eval => "eval",
            Self::Other => "autre",
        }
    }

    pub fn is_primary(self) -> bool {
        matches!(self, Self::Sources)
    }

    pub fn allowed_in_production(self) -> bool {
        self != Self::Eval
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_path_component() {
        assert_eq!(Collection::of(Path::new("/n/sources/a.pdf")), Collection::Sources);
        assert_eq!(Collection::of(Path::new("/n/wiki/a.md")), Collection::Wiki);
        assert_eq!(Collection::of(Path::new("/n/eval/q.md")), Collection::Eval);
        assert_eq!(Collection::of(Path::new("/n/divers/a.md")), Collection::Other);
    }

    #[test]
    fn substring_is_not_a_match() {
        assert_eq!(Collection::of(Path::new("/n/evaluation/a.md")), Collection::Other);
        assert_eq!(Collection::of(Path::new("/n/sources_old/a.md")), Collection::Other);
    }

    #[test]
    fn eval_is_never_allowed_in_production() {
        assert!(!Collection::Eval.allowed_in_production());
        assert!(Collection::Wiki.allowed_in_production());
        assert!(!Collection::Wiki.is_primary());
    }
}
