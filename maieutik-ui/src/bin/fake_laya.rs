//! Double de test : serveur MCP stdio imitant les sorties de `laya-primitives`.
//! `FAKE_LAYA_NOISE=1` : ligne non-JSON sur stdout au démarrage ;
//! `FAKE_LAYA_DELAY_MS` : retarde chaque réponse d'outil.

use serde_json::{json, Value};
use std::io::{BufRead, Write};

fn main() {
    if std::env::var("FAKE_LAYA_NOISE").is_ok() {
        println!("\u{1b}[2m2026-01-01T00:00:00Z\u{1b}[0m  INFO fake: Starting fake MCP server...");
    }
    let delay: u64 = std::env::var("FAKE_LAYA_DELAY_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(req) = serde_json::from_str::<Value>(&line) else { continue };
        let Some(id) = req.get("id").cloned() else { continue };
        let method = req["method"].as_str().unwrap_or("");
        let result = match method {
            "initialize" => json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "fake-laya", "version": "0.0.1"}
            }),
            "tools/list" => json!({"tools": [{"name": "health"}, {"name": "noul"}, {"name": "choice"}, {"name": "score"}]}),
            "tools/call" => {
                if delay > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                }
                let name = req["params"]["name"].as_str().unwrap_or("");
                let args = &req["params"]["arguments"];
                match tool(name, args) {
                    Ok(v) => json!({"content": [{"type": "text", "text": v.to_string()}], "isError": false}),
                    Err(m) => json!({"content": [{"type": "text", "text": m}], "isError": true}),
                }
            }
            _ => {
                let e = json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "method not found"}});
                writeln!(out, "{e}").ok();
                out.flush().ok();
                continue;
            }
        };
        writeln!(out, "{}", json!({"jsonrpc": "2.0", "id": id, "result": result})).ok();
        out.flush().ok();
    }
}

fn tool(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "health" => Ok(json!({"status": "healthy", "encoder_loaded": true,
            "noul_loaded": true, "choice_loaded": true, "score_loaded": true})),
        "noul" => {
            let t = args["text"].as_str().unwrap_or("").to_lowercase();
            let bad = t.contains("ignore previous instructions");
            Ok(json!({"is_safe": !bad, "injection_score": if bad {0.99} else {0.0},
                "null_score": 0.0, "is_ood": false, "ood_score": 0.0, "latency_ms": 1,
                "cls_embedding": [0.1, 0.2]}))
        }
        "choice" => {
            let opts: Vec<String> = args["options"].as_array().ok_or("options manquantes")?
                .iter().filter_map(|o| o.as_str().map(str::to_owned)).collect();
            if opts.is_empty() { return Err("options vides".into()); }
            Ok(json!({"selected_index": 0, "selected_option": opts[0],
                "probabilities": vec![1.0 / opts.len() as f64; opts.len()],
                "entropy": 0.1, "confidence": 0.9, "latency_ms": 1}))
        }
        "score" => {
            let chunks = args["chunks"].as_array().ok_or("chunks manquants")?;
            let q = args["query"].as_str().unwrap_or("").to_lowercase();
            let scores: Vec<Value> = chunks.iter().enumerate().map(|(i, c)| {
                let c = c.as_str().unwrap_or("");
                let rel = if c.to_lowercase().contains(&q) { 0.9 } else { 0.1 };
                json!({"chunk_index": i, "text_preview": c, "relevance": rel, "is_relevant": rel > 0.5})
            }).collect();
            Ok(json!({"scores": scores, "latency_ms": 1, "query_embedding": [0.1],
                "chunks_evaluated": chunks.len(), "chunks_relevant": 1}))
        }
        other => Err(format!("outil inconnu : {other}")),
    }
}
