use maieutik::utils::extract_text;
use std::path::Path;

#[test]
fn test_extract_text_from_sample_pdf() {
    let samples = vec![
        "/run/media/chelmooz/SAVE/Livres AI/AI Engineering Guidebook.pdf",
        "/run/media/chelmooz/SAVE/Livres AI/Chip Huyen - Designing Machine Learning Systems (2022, O_Reilly Media, Inc.) - libgen.li.pdf",
    ];

    for path in samples {
        if Path::new(path).exists() {
            let result = extract_text(Path::new(path));
            assert!(result.is_ok(), "Failed to extract {}", path);
            let text = result.unwrap();
            assert!(!text.is_empty(), "Empty text for {}", path);
        }
    }
}
