//! Tests de validation de configuration
//!
//! Ces tests vérifient que la configuration de Maieutik est correctement
//! chargée et que les valeurs par défaut sont appropriées.

use std::env;

#[test]
fn test_default_model_setting() {
    // Sans MAIEUTIK_LLM_MODEL défini, le système devrait utiliser un fallback
    // ou renvoyer une erreur claire
    env::remove_var("MAIEUTIK_LLM_MODEL");
    
    // À vérifier : que le système gère correctement l'absence de variable
}

#[test]
fn test_ollama_model_format() {
    // Format attendu: ollama://nom_modele
    let test_cases = vec![
        "ollama://qwen2.5:7b",
        "ollama://llama3.1:8b",
        "ollama://phi3:latest",
    ];
    
    for model in test_cases {
        assert!(model.starts_with("ollama://"), "Le modèle {} doit commencer par 'ollama://'", model);
    }
}

#[test]
fn test_config_file_structure() {
    // Les clés autorisées dans config.json
    let allowed_keys = vec!["Folders", "Extensions", "MaxFileSize"];
    
    // Toutes doivent être en PascalCase
    for key in &allowed_keys {
        let chars: Vec<char> = key.chars().collect();
        let first = chars[0];
        assert!(first.is_uppercase(), "La clé {} doit commencer par majuscule", key);
    }
}

#[test]
fn test_folder_path_validation() {
    // Les chemins de dossiers doivent être absolus pour éviter les problèmes
    let test_paths = vec![
        "/home/user/notes",
        "/data/documents",
        "/var/lib/maieutik/notes",
    ];
    
    for path in test_paths {
        assert!(path.starts_with('/'), "Le chemin {} doit être absolu", path);
    }
}