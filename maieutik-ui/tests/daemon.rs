//! Client socket Maieutik contre un serveur de test à la même trame.

use maieutik_ui::daemon::DaemonClient;
use maieutik_ui::ports::*;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

fn sock(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("maieutik-ui-test-{}-{name}.sock", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

fn serve(path: &PathBuf, n: usize, handler: fn(&str) -> Vec<u8>) -> thread::JoinHandle<()> {
    let l = UnixListener::bind(path).unwrap();
    thread::spawn(move || {
        for _ in 0..n {
            let (mut s, _) = l.accept().unwrap();
            let mut len = [0u8; 4];
            s.read_exact(&mut len).unwrap();
            let mut buf = vec![0u8; u32::from_le_bytes(len) as usize];
            s.read_exact(&mut buf).unwrap();
            let req: Value = serde_json::from_slice(&buf).unwrap();
            let body = handler(req["method"].as_str().unwrap());
            s.write_all(&(body.len() as u32).to_le_bytes()).unwrap();
            s.write_all(&body).unwrap();
        }
    })
}

fn ok_or_refuse(method: &str) -> Vec<u8> {
    let v = match method {
        "health.check" => json!({"ok": true, "result": {"version": "0.1.0"}, "id": 1}),
        other => json!({"ok": false, "error": format!("méthode inconnue ou non implémentée : {other}"), "id": 1}),
    };
    serde_json::to_vec(&v).unwrap()
}

#[test]
fn health_check_returns_version() {
    let p = sock("health");
    let h = serve(&p, 1, ok_or_refuse);
    assert_eq!(DaemonClient::new(p, Duration::from_secs(2)).health_check().unwrap(), "0.1.0");
    h.join().unwrap();
}

#[test]
fn unrouted_search_maps_to_not_wired() {
    let p = sock("search");
    let h = serve(&p, 1, ok_or_refuse);
    let err = DaemonClient::new(p, Duration::from_secs(2)).search("contrat", 3).unwrap_err();
    assert!(matches!(err, PortError::NotWired(_)), "reçu : {err:?}");
    h.join().unwrap();
}

#[test]
fn routed_search_parses_hits_and_derives_collection() {
    fn routed(_: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({"ok": true, "id": 1, "result": {"hits": [
            {"file_path": "/n/wiki/a.md", "text": "synthèse", "score": 0.4, "heading_path": "A > B"},
            {"file_path": "/n/eval/q.md", "text": "réponse attendue", "score": 0.1}
        ]}})).unwrap()
    }
    let p = sock("routed");
    let h = serve(&p, 1, routed);
    let hits = DaemonClient::new(p, Duration::from_secs(2)).search("q", 5).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].heading_path.as_deref(), Some("A > B"));
    assert_eq!(hits[0].collection.label(), "wiki");
    assert_eq!(hits[1].collection.label(), "eval");
    // Le pont renvoie une similarité normalisée : 1 = meilleur.
    assert_eq!(hits[0].score, 0.4);
    assert!(hits[0].score > hits[1].score);
    h.join().unwrap();
}

#[test]
fn search_mode_selects_the_bridge_method() {
    use std::sync::{Arc, Mutex};
    fn recording(path: &PathBuf, seen: Arc<Mutex<Vec<String>>>) -> thread::JoinHandle<()> {
        let l = UnixListener::bind(path).unwrap();
        thread::spawn(move || {
            for _ in 0..3 {
                let (mut s, _) = l.accept().unwrap();
                let mut len = [0u8; 4];
                s.read_exact(&mut len).unwrap();
                let mut buf = vec![0u8; u32::from_le_bytes(len) as usize];
                s.read_exact(&mut buf).unwrap();
                let req: Value = serde_json::from_slice(&buf).unwrap();
                seen.lock().unwrap().push(req["method"].as_str().unwrap().to_string());
                let body = serde_json::to_vec(&json!({"ok": true, "id": 1, "result": {"hits": []}})).unwrap();
                s.write_all(&(body.len() as u32).to_le_bytes()).unwrap();
                s.write_all(&body).unwrap();
            }
        })
    }
    let p = sock("modes");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let h = recording(&p, seen.clone());
    for m in [SearchMode::Semantic, SearchMode::Lexical, SearchMode::Hybrid] {
        DaemonClient::new(p.clone(), Duration::from_secs(2)).with_mode(m).search("q", 3).unwrap();
    }
    h.join().unwrap();
    assert_eq!(
        *seen.lock().unwrap(),
        vec!["search.semantic", "search.keyword", "search.hybrid"]
    );
}

#[test]
fn absent_socket_is_unavailable() {
    let err = DaemonClient::new(sock("absent"), Duration::from_secs(1)).health_check().unwrap_err();
    assert!(matches!(err, PortError::Unavailable(_)), "reçu : {err:?}");
}

#[test]
fn oversized_frame_is_refused() {
    let p = sock("big");
    let l = UnixListener::bind(&p).unwrap();
    let h = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut len = [0u8; 4];
        s.read_exact(&mut len).unwrap();
        let mut buf = vec![0u8; u32::from_le_bytes(len) as usize];
        s.read_exact(&mut buf).unwrap();
        s.write_all(&(2_000_000u32).to_le_bytes()).unwrap();
    });
    let err = DaemonClient::new(p, Duration::from_secs(2)).health_check().unwrap_err();
    assert!(matches!(err, PortError::Protocol(_)), "reçu : {err:?}");
    h.join().unwrap();
}
