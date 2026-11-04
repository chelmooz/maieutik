//! Noul Guardrail — Détection d'injection et validation de sécurité

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Severity {
    Safe,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    pub severity: Severity,
    pub reason: String,
}

pub fn analyze(prompt: &str) -> Analysis {
    let prompt_lower = prompt.to_lowercase();
    
    // Patterns critiques (injection SQL, jailbreak, etc.)
    let critical_patterns = [
        "ignore previous",
        "drop table",
        "delete from",
        "pwned",
        "jailbreak",
        "sudo",
        "rm -rf",
        "/etc/passwd",
    ];
    
    for pattern in &critical_patterns {
        if prompt_lower.contains(pattern) {
            return Analysis {
                severity: Severity::Critical,
                reason: format!("Pattern critique détecté: {}", pattern),
            };
        }
    }
    
    // Vide ou quasi-vide
    if prompt.trim().is_empty() {
        return Analysis {
            severity: Severity::Warning,
            reason: "Entrée vide ou invalide".to_string(),
        };
    }
    
    Analysis {
        severity: Severity::Safe,
        reason: "Entrée valide".to_string(),
    }
}

pub fn guard(payload: &str) -> Analysis {
    analyze(payload)
}
