//! MCP stdio transport test — JSON-RPC 2.0 over newline-delimited stdin/stdout.
//!
//! Contrat : le binaire lancé avec `--mcp-stdio` doit répondre aux requêtes
//! JSON-RPC 2.0 envoyées sur stdin, et écrire les réponses sur stdout.
//! Aucune trame de longueur n'est utilisée (contrairement au socket Unix).

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn start_mcp_stdio() -> (Command, std::process::Child) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_maieutik"));
    cmd.arg("--mcp-stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = cmd.spawn().expect("échec lancement maieutik --mcp-stdio");
    (cmd, child)
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
    let (_, mut child) = start_mcp_stdio();

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
    let (_, mut child) = start_mcp_stdio();
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
