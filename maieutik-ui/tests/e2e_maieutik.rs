//! Bout en bout : le client socket de l'UI parle au **vrai** pont MCP de
//! Maieutik (`McpServer::with_state` sur un socket Unix encadré), avec un
//! index réel à `HashEmbedder` — aucun modèle ONNX requis.
//!
//! Ce test est la couture entre les deux crates : il prouve que les trois
//! modes (`semantic`, `keyword`, `hybrid`) sont routés et que la forme des
//! hits (`file_path`, `text`, `score` ∈ [0,1], `heading_path`) est celle que
//! l'UI consomme.

use maieutik::config::FileContent;
use maieutik::core::indexer::{extract_headings, Chunker, HashEmbedder, VectorIndex};
use maieutik::mcp::McpServer;
use maieutik::AppState;
use maieutik_ui::daemon::DaemonClient;
use maieutik_ui::ports::{SearchMode, SearchPort};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

async fn wait_for_socket(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(path.exists(), "socket non créé : {}", path.display());
}

#[tokio::test(flavor = "multi_thread")]
async fn ui_client_gets_hits_from_the_real_mcp_bridge() {
    // Pas de `tempfile::tempdir()` : le serveur refuse un socket dont un
    // ancêtre est monde-écrivable, et `/tmp` l'est.
    let dir = tempfile::Builder::new()
        .prefix("e2e-maieutik-")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap();
    let index = VectorIndex::open(dir.path(), HashEmbedder::new()).unwrap();
    for (name, text) in [("fox.md", FOX), ("rust.md", RUST), ("bread.md", BREAD)] {
        index
            .upsert_file(Path::new(name), &content(name, text))
            .await
            .unwrap();
    }
    let state = AppState {
        config: Arc::new(
            maieutik::config::store::ConfigStore::new(None)
                .await
                .unwrap(),
        ),
        vector_index: Some(Arc::new(index)),
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

    for mode in [
        SearchMode::Semantic,
        SearchMode::Lexical,
        SearchMode::Hybrid,
    ] {
        let mut client = DaemonClient::new(sock.clone(), Duration::from_secs(5)).with_mode(mode);
        let hits = client.search("rust cargo compiler", 3).expect("recherche");
        assert!(!hits.is_empty(), "{mode:?} : aucun passage");
        assert!(
            hits.iter().any(|h| h.file_path == "rust.md"),
            "{mode:?} : la note rust.md devrait sortir — {hits:#?}"
        );
        assert!(
            hits.iter().all(|h| (0.0..=1.0).contains(&h.score)),
            "{mode:?} : score hors [0,1] — {hits:#?}"
        );
    }
}
