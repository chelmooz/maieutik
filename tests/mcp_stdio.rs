//! MCP stdio transport test — JSON-RPC 2.0 over newline-delimited stdin/stdout.
//!
//! Contrat : le binaire lancé avec `--mcp-stdio` doit répondre aux requêtes
//! JSON-RPC 2.0 envoyées sur stdin, et écrire les réponses sur stdout.
//! Aucune trame de longueur n'est utilisée (contrairement au socket Unix).

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn start_mcp_stdio(instance: &str) -> (tempfile::TempDir, Command, std::process::Child) {
    // Bac à sable : sans isolation, le serveur lirait le config.json réel de
    // l'utilisateur (et partagerait le verrou d'instance global).
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let home = dir.path().join("home");
    let cfg = dir.path().join("cfg");
    let data = dir.path().join("data");
    let cache = dir.path().join("cache");
    for d in [&home, &cfg, &data, &cache] {
        std::fs::create_dir_all(d).unwrap();
    }
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_maieutik"));
    cmd.arg("--mcp-stdio")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &cfg)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_CACHE_HOME", &cache)
        .env("MAIEUTIK_INSTANCE_NAME", instance)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = cmd.spawn().expect("échec lancement maieutik --mcp-stdio");
    (dir, cmd, child)
}

fn send_request(child: &mut std::process::Child, req: serde_json::Value) -> serde_json::Value {
    let stdin = child.stdin.as_mut().expect("stdin");
    let line = serde_json::to_string(&req).unwrap()
        + "
";
    stdin.write_all(line.as_bytes()).unwrap();
    stdin.flush().unwrap();

    let stdout = child.stdout.as_mut().expect("stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

#[tokio::test]
async fn stdio_transport_responds_to_health_check() {
    let (_sandbox, _, mut child) = start_mcp_stdio("test-mcp-stdio-health");

    // Wait a bit for server to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send initialize (required by MCP protocol)
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }
    });
    let resp = send_request(&mut child, init);
    assert!(
        resp.get("result").is_some(),
        "initialize doit réussir: {:?}",
        resp
    );

    // Send health.check
    let health = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "health.check",
        "params": {}
    });
    let resp = send_request(&mut child, health);
    assert!(
        resp.get("result").is_some(),
        "health.check doit réussir: {:?}",
        resp
    );
    assert_eq!(resp["result"]["version"], env!("CARGO_PKG_VERSION"));

    // Cleanup
    child.kill().unwrap();
}

#[tokio::test]
async fn stdio_transport_rejects_unknown_method() {
    let (_sandbox, _, mut child) = start_mcp_stdio("test-mcp-stdio-unknown");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "unknown.method",
        "params": {}
    });
    let resp = send_request(&mut child, req);
    assert!(
        !resp["error"].is_null(),
        "méthode inconnue doit échouer: {:?}",
        resp
    );

    child.kill().unwrap();
}
