use axum::{Json, extract::Query};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct Health { pub status: &'static str }

pub async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[derive(Deserialize)]
pub struct SearchParams { pub query: String }

#[derive(Serialize)]
pub struct SearchResult { pub query: String, pub hits: Vec<String> }

pub async fn list_files() -> Json<Vec<String>> {
    Json(vec!["example.md".into(), "notes.txt".into()])
}

pub async fn search(Query(params): Query<SearchParams>) -> Json<SearchResult> {
    Json(SearchResult { query: params.query, hits: vec![] })
}
