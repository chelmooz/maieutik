//! Tests du chunker et de l'extraction de titres (lot P1.8).
//!
//! Aucun modèle requis : ces tests tournent en CI. Les positions sont des
//! indices d'octets valides ; la taille et le recouvrement sont en caractères.

use std::path::PathBuf;

use maieutik::config::FileContent;
use maieutik::core::indexer::{extract_headings, Chunker};
use proptest::prelude::*;

fn content(text: &str) -> FileContent {
    FileContent {
        path: PathBuf::from("test.md"),
        text: text.to_string(),
        headings: extract_headings(text),
        chunks: Vec::new(),
    }
}

#[test]
fn reported_panic_case_does_not_panic() {
    // Cas relevé dans VERIFICATION.md : 12 × « é », taille 10, recouvrement 3.
    let text = "é".repeat(12);
    let chunks = Chunker::new(10, 3).chunk(&content(&text));
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].text.chars().count(), 10);
    assert_eq!(chunks[1].text.chars().count(), 5);
}

#[test]
fn size_and_overlap_are_in_characters() {
    let text = "é".repeat(40);
    let chunks = Chunker::new(10, 3).chunk(&content(&text));
    assert!(chunks.iter().all(|c| c.text.chars().count() <= 10));
    assert_eq!(chunks[0].byte_start, 0);
    assert_eq!(chunks[0].byte_end, 20);
    // Le deuxième chunk reprend 3 caractères (6 octets) : début à 7 × 2 octets.
    assert_eq!(chunks[1].byte_start, 14);
}

#[test]
fn emoji_and_cjk_slices_match_offsets() {
    for text in ["😀".repeat(25), "漢字かな".repeat(9), "aé😀漢".repeat(13)] {
        let chunks = Chunker::new(7, 2).chunk(&content(&text));
        assert!(!chunks.is_empty());
        for c in &chunks {
            assert_eq!(&text[c.byte_start..c.byte_end], c.text);
        }
        assert_eq!(chunks.last().unwrap().byte_end, text.len());
    }
}

#[test]
fn crlf_heading_offsets_are_exact() {
    let text = "# A\r\ntexte é\r\n## B\r\nfin\r\n";
    let headings = extract_headings(text);
    assert_eq!(headings.len(), 2);
    assert_eq!(headings[0].byte_offset, 0);
    assert_eq!(headings[0].line_number, 1);
    assert_eq!(headings[1].line_number, 3);
    assert!(text[headings[1].byte_offset..].starts_with("## B"));
}

#[test]
fn heading_extraction_survives_crlf_with_multibyte() {
    // L'ancien calcul d'offset tranchait au milieu d'un caractère.
    let text = "é\r\né\r\né\r\n# X\r\n";
    let headings = extract_headings(text);
    assert_eq!(headings.len(), 1);
    assert!(text[headings[0].byte_offset..].starts_with("# X"));
}

#[test]
fn heading_path_follows_the_text() {
    let text = "# A\nxxxxxxxxxx\n# B\nyyyyyyyyyy\n";
    let chunks = Chunker::new(8, 0).chunk(&content(text));
    assert_eq!(chunks[0].heading_path.as_deref(), Some("A"));
    assert_eq!(chunks.last().unwrap().heading_path.as_deref(), Some("B"));
}

#[test]
fn degenerate_parameters_terminate() {
    assert_eq!(Chunker::new(0, 5).chunk(&content("abcdef")).len(), 6);
    assert_eq!(Chunker::new(3, 99).chunk(&content("abcdef")).len(), 4);
    assert!(Chunker::new(3, 1).chunk(&content("")).is_empty());
}

#[test]
fn ascii_without_overlap_keeps_previous_chunks() {
    // Valeurs relevées sur l'ancien chunker : le texte des chunks ASCII sans
    // recouvrement ne change pas, donc les embeddings de test non plus.
    let text = "# T\nabcdefghijklmnopqrstuvwxyz\n".repeat(2);
    let got: Vec<String> = Chunker::new(16, 0)
        .chunk(&content(&text))
        .into_iter()
        .map(|c| c.text)
        .collect();
    assert_eq!(
        got,
        [
            "# T\nabcdefghijkl",
            "mnopqrstuvwxyz\n#",
            " T\nabcdefghijklm",
            "nopqrstuvwxyz\n"
        ]
    );
}

proptest! {
    #[test]
    fn chunks_cover_text_with_valid_offsets(
        text in "\\PC{0,300}",
        max in 1usize..40,
        overlap in 0usize..60,
    ) {
        let chunks = Chunker::new(max, overlap).chunk(&content(&text));
        if text.is_empty() {
            prop_assert!(chunks.is_empty());
        } else {
            prop_assert_eq!(chunks[0].byte_start, 0);
            prop_assert_eq!(chunks.last().unwrap().byte_end, text.len());
            for (i, c) in chunks.iter().enumerate() {
                prop_assert_eq!(c.index, i);
                prop_assert_eq!(&text[c.byte_start..c.byte_end], c.text.as_str());
                prop_assert!(c.text.chars().count() <= max);
                if let Some(next) = chunks.get(i + 1) {
                    prop_assert_eq!(c.text.chars().count(), max);
                    prop_assert!(next.byte_start > c.byte_start);
                    prop_assert!(next.byte_start <= c.byte_end);
                }
            }
        }
    }

    #[test]
    fn headings_start_on_a_hash(text in "(#{0,8} ?[a-zé😀]{0,5}(\r\n|\n|\r)?){0,20}") {
        for h in extract_headings(&text) {
            prop_assert!(text.is_char_boundary(h.byte_offset));
            prop_assert!(text[h.byte_offset..].starts_with('#'));
            prop_assert!((1..=6).contains(&h.level));
        }
    }
}
