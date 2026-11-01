//! Régression PDF : un document hostile ne doit jamais tuer le processus.
//!
//! RUSTSEC-2026-0187 : `lopdf` < 0.42 (via `pdf-extract`) déborde sa pile sur
//! des objets PDF profondément imbriqués — l'abort ne peut pas être intercepté
//! par un `catch_unwind`. Le contrat : `extract_text` retourne toujours (Ok ou
//! Err), quel que soit le contenu du fichier ; l'indexation d'un PDF hostile
//! dégrade au pire ce fichier, jamais le service entier.
//!
//! La fixture est générée dans le test (hermétique, sans données personnelles) :
//! 200 000 tableaux imbriqués, plus la table xref exacte pour que l'analyseur
//! descende réellement la structure.

use maieutik::utils::extract_text;
use std::io::Write;

/// PDF minimal dont l'objet 1 est un tableau profondément imbriqué, référencé
/// comme /Root dans le trailer.
fn deeply_nested_pdf(depth: usize) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n");
    let obj1_off = out.len();
    out.extend_from_slice(b"1 0 obj\n");
    out.resize(out.len() + depth, b'[');
    for _ in 0..depth {
        out.extend_from_slice(b" 1]");
    }
    out.extend_from_slice(b"\nendobj\n");
    let xref_off = out.len();
    out.extend_from_slice(
        format!(
            "xref\n0 2\n0000000000 65535 f \n{:010} 00000 n \n",
            obj1_off
        )
        .as_bytes(),
    );
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            xref_off
        )
        .as_bytes(),
    );
    out
}

#[test]
fn corrupted_pdf_is_handled_gracefully() {
    let dir = tempfile::tempdir().unwrap();
    let pdf = dir.path().join("corrupted.pdf");
    // Write invalid PDF content
    std::fs::write(
        &pdf,
        b"%PDF-1.7
not a valid pdf",
    )
    .unwrap();
    // Should return an error, not panic
    assert!(extract_text(&pdf).is_err());
}

#[test]
fn deeply_nested_pdf_never_kills_the_process() {
    let dir = tempfile::tempdir().unwrap();
    let pdf = dir.path().join("imbrique.pdf");
    let bytes = deeply_nested_pdf(200_000);
    std::fs::File::create(&pdf)
        .unwrap()
        .write_all(&bytes)
        .unwrap();

    // Avec lopdf 0.34 (pré-correction), cet appel abort le processus :
    // stack overflow non interceptable. Le test est rouge par crash.
    let text = extract_text(&pdf).expect("l'extraction doit retourner, pas crasher");

    // L'arbre de pages est un tableau : aucune page, donc aucun texte.
    assert!(
        text.len() < 1_000,
        "aucun texte ne doit sortir d'un PDF sans page ; obtenu : {:?}",
        text.chars().take(80).collect::<String>()
    );
}
