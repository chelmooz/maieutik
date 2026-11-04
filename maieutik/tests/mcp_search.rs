//! Recherche par le pont MCP : un client local doit obtenir des passages via
//! `search.hybrid` / `search.keyword` / `search.semantic` sur le socket Unix
//! encadré, avec les filtres `folders` et `min_score`.
//!
//! Contrat testé au seam du protocole : serveur réel (`McpServer::with_state`)
//! lancé sur un socket de bac à sable, client `UnixStream` réel, trames
//! écrites/lues via `framed`. `HashEmbedder` remplace le modèle ONNX : aucun
//! téléchargement n'est requis.

use maieutik::config::store::ConfigStore;
use maieutik::config::FileContent;
use maieutik::core::indexer::{extract_headings, Chunker, HashEmbedder, VectorIndex};
use maieutik::mcp::framed::{self, RpcRequest, RpcResponse};
use maieutik::mcp::McpServer;
use maieutik::AppState;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UnixStream;

const FOX: &str = "the quick brown fox jumps over the lazy dog near the river";
const RUST: &str = "rust cargo compiler borrow checker lifetimes traits";
const BREAD: &str = "bread flour yeast oven baking sourdough starter";

fn content(path: &str, text: &str) -> FileContent {
    let base = FileContent {
        path: std::path::PathBuf::from(path),
        text: text.to_string(),
        headings: extract_headings(text),
        chunks: Vec::new(),
    };
    let chunks = Chunker::new(1000, 0).chunk(&base);
    FileContent { chunks, ..base }
}

/// Indexe les trois notes de référence dans un répertoire de bac à sable.
async fn seeded_index(dir: &Path) -> Arc<VectorIndex> {
    let index = VectorIndex::open(dir, HashEmbedder::new()).unwrap();
    for (name, text) in [("fox.md", FOX), ("rust.md", RUST), ("bread.md", BREAD)] {
        index
            .upsert_file(Path::new(name), &content(name, text))
            .await
            .unwrap();
    }
    Arc::new(index)
}

/// Attend que le serveur ait créé le socket (bind async), au plus 5 s.
async fn wait_for_socket(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        path.exists(),
        "le serveur doit créer son socket : {}",
        path.display()
    );
}

/// Serveur MCP relié à un index réel, sur un socket de bac à sable.
async fn server_on_sandbox_socket() -> (tempfile::TempDir, std::path::PathBuf) {
    // Pas de `tempfile::tempdir()` : le serveur refuse un socket dont un
    // ancêtre est monde-écrivable, et `/tmp` l'est. Le bac à sable vit donc
    // dans le crate (0700), comme dans `mcp_health.rs`.
    let dir = tempfile::Builder::new()
        .prefix("mcp-search-")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap();
    let index = seeded_index(dir.path()).await;
    let config = Arc::new(ConfigStore::new(None).await.unwrap());
    let state = AppState {
        config,
        vector_index: Some(index),
        embedder: None,
    };
    let sock = dir.path().join("mcp.sock");
    let sock_str = sock.to_str().unwrap().to_string();
    let server = Arc::new(McpServer::with_state(Arc::new(state)));
    tokio::spawn(async move {
        // Boucle d'accept infinie : le test garde la main sur l'arrêt.
        let _ = server.run_unix_socket(&sock_str).await;
    });
    wait_for_socket(&sock).await;
    (dir, sock)
}

async fn request(sock: &Path, method: &str, params: serde_json::Value) -> RpcResponse {
    let mut stream = UnixStream::connect(sock)
        .await
        .expect("connexion au socket MCP");
    framed::write_request(
        &mut stream,
        &RpcRequest {
            method: method.to_string(),
            params,
            id: Some(serde_json::json!(1)),
        },
    )
    .await
    .expect("écriture de la requête encadrée");
    framed::read_response(&mut stream)
        .await
        .expect("lecture de la réponse encadrée")
        .expect("le serveur doit répondre avant de fermer la connexion")
}

/// Extrait `hits[].file_path` d'une réponse de recherche.
fn hit_paths(resp: &RpcResponse) -> Vec<String> {
    resp.result
        .as_ref()
        .and_then(|r| r.get("hits"))
        .and_then(|hits| hits.as_array())
        .map(|hits| {
            hits.iter()
                .filter_map(|hit| hit.get("file_path").and_then(|p| p.as_str()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn hybrid_search_returns_the_matching_note() {
    let (_dir, sock) = server_on_sandbox_socket().await;
    let resp = request(
        &sock,
        "search.hybrid",
        serde_json::json!({ "query": "rust borrow checker", "limit": 3 }),
    )
    .await;

    assert!(resp.ok, "search.hybrid doit réussir ; {:?}", resp.error);
    let paths = hit_paths(&resp);
    assert_eq!(
        paths.first().map(String::as_str),
        Some("rust.md"),
        "{paths:?}"
    );
}

#[tokio::test]
async fn keyword_search_matches_literal_terms() {
    let (_dir, sock) = server_on_sandbox_socket().await;
    let resp = request(
        &sock,
        "search.keyword",
        serde_json::json!({ "query": "sourdough starter", "limit": 3 }),
    )
    .await;

    assert!(resp.ok, "search.keyword doit réussir ; {:?}", resp.error);
    assert_eq!(
        hit_paths(&resp).first().map(String::as_str),
        Some("bread.md")
    );
}

#[tokio::test]
async fn semantic_search_still_answers() {
    let (_dir, sock) = server_on_sandbox_socket().await;
    let resp = request(
        &sock,
        "search.semantic",
        serde_json::json!({ "query": "bread flour yeast", "limit": 3 }),
    )
    .await;

    assert!(resp.ok, "search.semantic doit réussir ; {:?}", resp.error);
    assert_eq!(
        hit_paths(&resp).first().map(String::as_str),
        Some("bread.md")
    );
}

#[tokio::test]
async fn folders_filter_excludes_other_notes() {
    let (_dir, sock) = server_on_sandbox_socket().await;
    let resp = request(
        &sock,
        "search.hybrid",
        serde_json::json!({
            "query": "rust borrow checker sourdough",
            "limit": 5,
            "folders": ["bread.md"]
        }),
    )
    .await;

    assert!(resp.ok, "le filtre ne doit pas échouer ; {:?}", resp.error);
    let paths = hit_paths(&resp);
    assert!(
        paths.iter().all(|p| p == "bread.md"),
        "seul bread.md est autorisé ; obtenu : {paths:?}"
    );
}

#[tokio::test]
async fn min_score_filter_is_applied() {
    let (_dir, sock) = server_on_sandbox_socket().await;
    // Un plancher à 1.0 ne laisse passer que le meilleur hit normalisé.
    let resp = request(
        &sock,
        "search.hybrid",
        serde_json::json!({ "query": "rust cargo compiler", "limit": 5, "min_score": 1.0 }),
    )
    .await;

    assert!(resp.ok, "le filtre ne doit pas échouer ; {:?}", resp.error);
    let paths = hit_paths(&resp);
    assert!(!paths.is_empty(), "le meilleur hit doit survivre au filtre");
    assert!(paths.iter().all(|p| p == "rust.md"), "{paths:?}");
}

#[tokio::test]
async fn limit_is_bounded_to_the_index_size() {
    let (_dir, sock) = server_on_sandbox_socket().await;
    let resp = request(
        &sock,
        "search.hybrid",
        serde_json::json!({ "query": "rust sourdough fox", "limit": 1000 }),
    )
    .await;

    assert!(resp.ok, "{:?}", resp.error);
    assert!(hit_paths(&resp).len() <= 3, "un seul chunk par note ici");
}

#[tokio::test]
async fn missing_index_is_reported_not_silently_empty() {
    let dir = tempfile::Builder::new()
        .prefix("mcp-search-noindex-")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap();
    let state = AppState {
        config: Arc::new(ConfigStore::new(None).await.unwrap()),
        vector_index: None,
        embedder: None,
    };
    let sock = dir.path().join("mcp.sock");
    let sock_str = sock.to_str().unwrap().to_string();
    let server = Arc::new(McpServer::with_state(Arc::new(state)));
    tokio::spawn(async move {
        let _ = server.run_unix_socket(&sock_str).await;
    });
    wait_for_socket(&sock).await;

    let resp = request(
        &sock,
        "search.hybrid",
        serde_json::json!({ "query": "rust" }),
    )
    .await;
    assert!(!resp.ok, "un index absent doit être une erreur explicite");
    assert!(
        resp.error.as_deref().unwrap_or_default().contains("index"),
        "{:?}",
        resp.error
    );
}

#[tokio::test]
async fn chat_complete_is_wired_and_answers() {
    let (_dir, sock) = server_on_sandbox_socket().await;
    let resp = request(
        &sock,
        "chat.complete",
        serde_json::json!({ "message": "bonjour" }),
    )
    .await;

    assert!(resp.ok, "chat.complete est branché ; {resp:?}");
    let result = resp.result.expect("résultat attendu");
    let answer = result
        .get("response")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(!answer.is_empty(), "la réponse ne doit pas être vide");
}
