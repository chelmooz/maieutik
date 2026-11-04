use axum::{Json, extract::Query, http::StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct Health { pub status: &'static str }

pub async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[derive(Deserialize)]
pub struct SearchParams { pub query: String }

#[derive(Serialize, Debug)]
pub struct SearchResult { pub query: String, pub hits: Vec<String> }

pub async fn list_files() -> Json<Vec<String>> {
    Json(vec!["example.md".into(), "notes.txt".into()])
}

#[cfg(test)]
mod tests;

pub async fn search(Query(params): Query<SearchParams>) -> Result<Json<SearchResult>, (StatusCode, Json<serde_json::Value>)> {
    // Simple Noul guardrail check - pattern based
    let lower = params.query.to_lowercase();
    let critical = ["ignore previous", "drop table", "delete from", "jailbreak", "pwned"];
    if critical.iter().any(|p| lower.contains(p)) {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Guard rejected"}))));
    }
    Ok(Json(SearchResult { query: params.query, hits: vec![] }))
}
