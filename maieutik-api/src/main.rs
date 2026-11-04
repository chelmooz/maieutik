use axum::{Router, routing::get};
use maieutik_api::routes::{health, list_files, search};

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/files", get(list_files))
        .route("/api/search", get(search));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("API running on http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}
