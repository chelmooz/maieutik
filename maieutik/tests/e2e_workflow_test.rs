//! Tests end-to-end sans modèle ONNX : fichiers réels → scan → index → recherche
//! hybride → pont MCP sur socket Unix, avec `HashEmbedder` comme substitut
//! déterministe de l'embedder BGE-M3.

use maieutik::config::store::ConfigStore;
use maieutik::config::AppSettings;
use maieutik::core::indexer::{HashEmbedder, VectorIndex};
use maieutik::core::retrieval::SearchMode;
use maieutik::core::scan::index_all;
use maieutik::mcp::framed::{self, RpcRequest, RpcResponse};
use maieutik::mcp::McpServer;
use maieutik::AppState;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::net::UnixStream;

const RUST_NOTE: &str = "# Rust\n\nRust enforces memory safety at compile time. \
The borrow checker rejects data races before the program runs.\n";
const BREAD_NOTE: &str = "# Sourdough\n\nA sourdough starter is a culture of wild yeast \
and lactic acid bacteria. Feeding it flour and water keeps it active.\n";
const UPDATED_NOTE: &str = "# Fermentation\n\nSourdough fermentation develops flavour. \
A slow cold retard makes the bread more sour.\n";

fn workspace() -> (TempDir, PathBuf, PathBuf) {
    let root = tempfile::Builder::new()
        .prefix("maieutik-e2e-")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .expect("répertoire temporaire");
    let notes = root.path().join("notes");
    let index = root.path().join("index");
    std::fs::create_dir_all(&notes).expect("dossier notes");
    std::fs::create_dir_all(&index).expect("dossier index");
    (root, notes, index)
}

fn settings_for(notes: &Path) -> AppSettings {
    let mut settings = AppSettings::default();
    settings.folders = vec![notes.to_string_lossy().into_owned()];
    settings.extensions = vec![".md".into()];
    settings
}

fn open_index(dir: &Path) -> Arc<VectorIndex> {
    Arc::new(VectorIndex::open(dir, HashEmbedder::new()).expect("ouverture de l'index"))
}

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

async fn mcp_request(sock: &Path, method: &str, params: serde_json::Value) -> RpcResponse {
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

fn hit_paths(response: &RpcResponse) -> Vec<String> {
    response
        .result
        .as_ref()
        .and_then(|result| result.get("hits"))
        .and_then(|hits| hits.as_array())
        .map(|hits| {
            hits.iter()
                .filter_map(|hit| hit.get("file_path").and_then(|path| path.as_str()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn files_scan_index_hybrid_search_and_mcp_are_connected() {
    let (_root, notes, index_dir) = workspace();
    std::fs::write(notes.join("rust.md"), RUST_NOTE).expect("écriture rust.md");
    std::fs::write(notes.join("bread.md"), BREAD_NOTE).expect("écriture bread.md");

    let settings = settings_for(&notes);
    let index = open_index(&index_dir);

    let report = index_all(&index, &settings, true)
        .await
        .expect("indexation du corpus");
    assert_eq!(report.scanned, 2, "deux notes doivent être parcourues");
    assert_eq!(report.indexed, 2, "deux notes doivent être indexées");
    assert!(report.failed.is_empty(), "échecs : {:?}", report.failed);
    assert!(
        report.no_text.is_empty(),
        "sans texte : {:?}",
        report.no_text
    );
    assert_eq!(report.exit_code(), 0);

    let hits = index
        .search_mode("rust borrow checker", 5, SearchMode::Hybrid)
        .await
        .expect("recherche hybride directe");
    assert!(
        !hits.is_empty(),
        "la recherche doit rendre au moins un passage"
    );
    assert!(
        hits[0].file_path.ends_with("rust.md"),
        "attendu rust.md en tête, obtenu {:?}",
        hits.iter().map(|hit| &hit.file_path).collect::<Vec<_>>()
    );

    let config = Arc::new(ConfigStore::new(None).await.expect("configuration"));
    let state = AppState {
        config,
        vector_index: Some(index),
        embedder: None,
    };
    let socket = index_dir.join("mcp.sock");
    let socket_str = socket.to_string_lossy().into_owned();
    let server = Arc::new(McpServer::with_state(Arc::new(state)));
    let server_task = tokio::spawn(async move {
        let _ = server.run_unix_socket(&socket_str).await;
    });
    wait_for_socket(&socket).await;

    let health = mcp_request(&socket, "health.check", serde_json::json!({})).await;
    assert!(health.ok, "health.check doit réussir : {:?}", health.error);

    let response = mcp_request(
        &socket,
        "search.hybrid",
        serde_json::json!({ "query": "sourdough starter", "limit": 5 }),
    )
    .await;
    assert!(
        response.ok,
        "search.hybrid doit réussir : {:?}",
        response.error
    );
    let paths = hit_paths(&response);
    assert!(!paths.is_empty(), "search.hybrid doit rendre un passage");
    assert!(
        paths[0].ends_with("bread.md"),
        "attendu bread.md via MCP, obtenu {paths:?}"
    );

    server_task.abort();
}

#[tokio::test]
async fn modifying_a_note_and_reindexing_updates_search_results() {
    let (_root, notes, index_dir) = workspace();
    let note = notes.join("note.md");
    std::fs::write(&note, RUST_NOTE).expect("écriture initiale");
    let settings = settings_for(&notes);
    let index = open_index(&index_dir);

    index_all(&index, &settings, false)
        .await
        .expect("indexation initiale");

    let before = index
        .search_mode("borrow checker", 5, SearchMode::Lexical)
        .await
        .expect("recherche avant modification");
    assert!(
        before.iter().any(|hit| hit.file_path.ends_with("note.md")),
        "le terme initial doit être trouvé"
    );

    std::fs::write(&note, UPDATED_NOTE).expect("écriture de la version modifiée");
    index_all(&index, &settings, false)
        .await
        .expect("réindexation après modification");

    let stale = index
        .search_mode("borrow checker", 5, SearchMode::Lexical)
        .await
        .expect("recherche du terme retiré");
    assert!(
        stale.is_empty(),
        "l'ancien contenu ne doit plus être indexé : {stale:?}"
    );

    let fresh = index
        .search_mode("sourdough fermentation", 5, SearchMode::Lexical)
        .await
        .expect("recherche du nouveau terme");
    assert!(
        fresh.iter().any(|hit| hit.file_path.ends_with("note.md")),
        "le nouveau contenu doit être trouvé : {fresh:?}"
    );
}

#[tokio::test]
async fn force_reindex_purges_notes_deleted_from_disk() {
    let (_root, notes, index_dir) = workspace();
    let rust_note = notes.join("rust.md");
    let bread_note = notes.join("bread.md");
    std::fs::write(&rust_note, RUST_NOTE).expect("écriture rust.md");
    std::fs::write(&bread_note, BREAD_NOTE).expect("écriture bread.md");

    let settings = settings_for(&notes);
    let index = open_index(&index_dir);
    index_all(&index, &settings, true)
        .await
        .expect("indexation initiale");

    std::fs::remove_file(&bread_note).expect("suppression bread.md");
    let report = index_all(&index, &settings, true)
        .await
        .expect("réindexation forcée");

    assert_eq!(report.indexed, 1, "seule rust.md reste à indexer");
    let stored = index.stored_file_paths().expect("chemins indexés");
    assert!(
        stored.iter().all(|path| !path.ends_with("bread.md")),
        "bread.md doit être purgé : {stored:?}"
    );

    let hits = index
        .search_mode("sourdough starter", 5, SearchMode::Lexical)
        .await
        .expect("recherche après purge");
    assert!(
        hits.is_empty(),
        "plus aucun passage de bread.md ne doit être retourné : {hits:?}"
    );
}
