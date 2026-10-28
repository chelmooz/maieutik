//! Health-check du pont MCP : un client local doit pouvoir demander
//! `health.check` sur le socket Unix encadré (préfixe de longueur 4 octets +
//! JSON) et recevoir un acquittement — pas un « Not implemented ».
//!
//! Contrat testé au seam du protocole : serveur réel lancé sur un socket de
//! bac à sable, client `UnixStream` réel, trames écrites/lues via `framed`.
//! Aucun modèle d'embedding requis.

use maieutik::mcp::framed::{self, RpcRequest, RpcResponse};
use maieutik::mcp::McpServer;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UnixStream;

/// Attend que le serveur ait créé le socket (bind async), au plus 5 s.
async fn wait_for_socket(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(path.exists(), "le serveur doit créer son socket : {}", path.display());
}

/// Ouvre le serveur sur un socket de bac à sable et retourne son chemin.
async fn server_on_sandbox_socket() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("mcp.sock");
    let server = Arc::new(McpServer::new());
    let sock_str = sock.to_str().unwrap().to_string();
    tokio::spawn(async move {
        // Boucle d'accept infinie : le test garde la main sur l'arrêt.
        let _ = server.run_unix_socket(&sock_str).await;
    });
    wait_for_socket(&sock).await;
    (dir, sock)
}

async fn request(sock: &std::path::Path, method: &str) -> RpcResponse {
    let mut stream = UnixStream::connect(sock).await.expect("connexion au socket MCP");
    framed::write_request(
        &mut stream,
        &RpcRequest {
            method: method.to_string(),
            params: serde_json::json!({}),
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

#[tokio::test]
async fn health_check_answers_ok_over_the_framed_socket() {
    let (_dir, sock) = server_on_sandbox_socket().await;
    let resp = request(&sock, "health.check").await;

    assert!(resp.ok, "health.check doit réussir ; obtenu : {:?}", resp.error);
    assert_eq!(resp.error, None, "aucune erreur attendue");
    let result = resp.result.expect("health.check renvoie un résultat");
    assert_eq!(
        result.get("version").and_then(|v| v.as_str()),
        Some(env!("CARGO_PKG_VERSION")),
        "le résultat porte la version du binaire ; obtenu : {result}"
    );
    assert_eq!(resp.id, Some(serde_json::json!(1)), "l'id fait l'aller-retour");
}

#[tokio::test]
async fn unknown_method_is_still_reported_as_not_implemented() {
    // Le health-check ne doit pas transformer le stub en serveur complet :
    // une méthode non branchée continue d'être refusée explicitement.
    let (_dir, sock) = server_on_sandbox_socket().await;
    let resp = request(&sock, "chat.complete").await;

    assert!(!resp.ok, "une méthode non branchée doit échouer ; obtenu : {resp:?}");
    assert!(
        resp.error.as_deref().unwrap_or_default().contains("implément"),
        "l'erreur doit le dire ; obtenu : {:?}",
        resp.error
    );
}
