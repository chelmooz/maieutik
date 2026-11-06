//! Entry point — serveur API Maieutik.
//!
//! Ce serveur est **en lecture seule** : il expose l'index existant via HTTP.
//! Avant de le démarrer, exécutez `maieutik index` pour créer l'index.

use axum::{routing::get, routing::post, Router};
use maieutik::AppState as MaieutikAppState;
use maieutik_api::app::ApiState;
use maieutik_api::routes::{chat, health, list_files, search};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Vérifier que l'index existe
    let data_dir = maieutik::config::store::data_dir()
        .map_err(|e| anyhow::anyhow!("Impossible de déterminer data_dir: {}", e))?;
    let hnsw_path = data_dir.join("hnsw.bin");

    if !hnsw_path.exists() {
        anyhow::bail!(
            "Index non trouvé à {}. Lancez 'maieutik index' d'abord.",
            hnsw_path.display()
        );
    }

    // Charger l'état maieutik existant
    let maieutik_state = MaieutikAppState::new()
        .await
        .map_err(|e| anyhow::anyhow!("Échec de l'initialisation: {}", e))?;
    let _vector_index = maieutik_state
        .vector_index
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("L'index n'a pas pu être ouvert."))?
        .clone();

    eprintln!("Index chargé avec succès");

    // Créer l'état API
    let api_state = ApiState::new(std::sync::Arc::new(maieutik_state));

    // Routes
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/files", get(list_files))
        .route("/api/search", get(search))
        .route("/chat", post(chat))
        .with_state(api_state);

    // Bind sur 127.0.0.1:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    eprintln!("API running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    // Serve until shutdown signal
    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("Échec du serveur: {}", e))?;

    Ok(())
}
