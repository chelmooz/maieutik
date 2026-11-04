use axum::{Router, routing::get, Json};
use serde_json::json;

async fn health() -> Json<serde_json::Value> {
    Json(json!({"status": "ok", "service": "maieutik-api"}))
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/health", get(health));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("API running on http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}
