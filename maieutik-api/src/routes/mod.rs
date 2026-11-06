//! API routes — adapteurs vers les ports métier.
//!
//! Chaque handler dépend d'injections (`State<T>`) conformément au principe
//! d'inversion de dépendance (DIP). La logique métier reste dans le domaine
//! `maieutik` (recherche, indexation).

use axum::{
    extract::{Query, State},
    Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::app::ApiState;
use crate::ports::{FileCatalog, Generator};

#[derive(Serialize)]
pub struct Health {
    pub status: &'static str,
}

pub async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub query: String,
}

#[derive(Serialize, Debug)]
pub struct SearchResult {
    pub query: String,
    pub hits: Vec<String>,
}

/// Liste les fichiers indexés via le `FileCatalog` port.
pub async fn list_files(State(state): State<ApiState>) -> Result<Json<Vec<String>>, (StatusCode, Json<serde_json::Value>)> {
    state
        .file_catalog
        .list()
        .await
        .map(Json)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })
}

/// Recherche sémantique en utilisant l'index vectoriel.
///
/// Le guardrail Noul est appliqué pour rejeter les prompts d'injection
/// de type "ignore previous instructions", "jailbreak", etc.
pub async fn search(
    State(state): State<ApiState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResult>, (StatusCode, Json<serde_json::Value>)> {
    // Noul guardrail — pattern based
    let lower = params.query.to_lowercase();
    let critical = [
        "ignore previous",
        "drop table",
        "delete from",
        "jailbreak",
        "pwned",
        "ignore.*instructions",
    ];
    if critical.iter().any(|p| lower.contains(p)) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Guard rejected"})),
        ));
    }

    // Utiliser le search_mode hybride disponible dans VectorIndex
    let hits = state
        .file_catalog
        .index
        .search_mode(&params.query, 20, maieutik::core::retrieval::SearchMode::Hybrid)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?
        .into_iter()
        .filter_map(|hit| {
            // Retourner le texte du chunk pour le frontend
            let preview = hit.text.chars().take(500).collect::<String>();
            if preview.len() < hit.text.len() || !preview.is_empty() {
                Some(format!("{}: {}", hit.file_path, preview))
            } else {
                None
            }
        })
        .collect();

    Ok(Json(SearchResult {
        query: params.query,
        hits,
    }))
}

/// Endpoint chat — délègue au `Generator` port.
///
/// Corps attendu : `{"messages": [{"role": "user", "content": "..."}]}`
#[derive(Deserialize)]
pub struct ChatRequest {
    messages: Vec<crate::ports::ChatMessage>,
}

pub async fn chat(
    State(state): State<ApiState>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match &state.generator {
        Some(gen) => {
            let response = gen
                .chat(req.messages)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": e.to_string()})),
                    )
                })?;

            Ok(Json(serde_json::json!({"message": {"role": "assistant", "content": response}})))
        }
        None => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Generator not configured (MAIEUTIK_LLM_MODEL not set)"})),
        )),
    }
}

#[cfg(test)]
mod tests;