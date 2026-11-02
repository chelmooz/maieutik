//! Adaptateur Maieutik : socket Unix local (trame 4 octets LE + JSON).
//! Requête `{method, params, id}`, réponse `{ok, result, error, id}`.
//! `health.check` et les trois recherches (`search.semantic`, `search.keyword`,
//! `search.hybrid`) sont routées côté serveur ; ce n'est PAS du JSON-RPC 2.0.

use crate::ports::*;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

const MAX_FRAME: usize = 1024 * 1024;

/// Même résolution que `framed::socket_path()` du serveur.
pub fn default_socket_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MAIEUTIK_MCP_SOCKET") {
        return Some(PathBuf::from(p));
    }
    if let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR") {
        return Some(PathBuf::from(rt).join("maieutik_mcp.sock"));
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(cache.join("maieutik").join("maieutik_mcp.sock"))
}

pub struct DaemonClient {
    path: PathBuf,
    timeout: Duration,
    next_id: u64,
    mode: SearchMode,
}

impl DaemonClient {
    pub fn new(path: PathBuf, timeout: Duration) -> Self {
        Self {
            path,
            timeout,
            next_id: 1,
            mode: SearchMode::default(),
        }
    }

    /// Choisit la méthode de recherche (`search.semantic` / `.keyword` / `.hybrid`).
    pub fn with_mode(mut self, mode: SearchMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, PortError> {
        let mut s = UnixStream::connect(&self.path)
            .map_err(|e| PortError::Unavailable(format!("socket {} : {e}", self.path.display())))?;
        s.set_read_timeout(Some(self.timeout)).ok();
        s.set_write_timeout(Some(self.timeout)).ok();
        let id = self.next_id;
        self.next_id += 1;
        let body = serde_json::to_vec(&json!({"method": method, "params": params, "id": id}))
            .map_err(|e| PortError::Io(e.to_string()))?;
        s.write_all(&(body.len() as u32).to_le_bytes())
            .and_then(|_| s.write_all(&body))
            .and_then(|_| s.flush())
            .map_err(|e| PortError::Io(e.to_string()))?;

        let mut len = [0u8; 4];
        s.read_exact(&mut len).map_err(map_read)?;
        let len = u32::from_le_bytes(len) as usize;
        if len > MAX_FRAME {
            return Err(PortError::Protocol(format!(
                "trame de {len} octets refusée"
            )));
        }
        let mut buf = vec![0u8; len];
        s.read_exact(&mut buf).map_err(map_read)?;
        let resp: Value =
            serde_json::from_slice(&buf).map_err(|e| PortError::Protocol(e.to_string()))?;
        if resp["ok"].as_bool().unwrap_or(false) {
            Ok(resp["result"].clone())
        } else {
            let msg = resp["error"]
                .as_str()
                .unwrap_or("erreur sans message")
                .to_owned();
            if msg.contains("non implémentée") {
                Err(PortError::NotWired(msg))
            } else {
                Err(PortError::Refused(msg))
            }
        }
    }

    pub fn health_check(&mut self) -> Result<String, PortError> {
        let v = self.call("health.check", json!({}))?;
        Ok(v["version"].as_str().unwrap_or("?").to_owned())
    }
}

fn map_read(e: std::io::Error) -> PortError {
    match e.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => PortError::Timeout(0),
        _ => PortError::Io(e.to_string()),
    }
}

impl SearchPort for DaemonClient {
    fn search(&mut self, query: &str, k: usize) -> Result<Vec<Hit>, PortError> {
        let v = self.call(self.mode.method(), json!({"query": query, "limit": k}))?;
        let hits = v["hits"]
            .as_array()
            .ok_or_else(|| PortError::Protocol("champ `hits` absent".into()))?;
        Ok(hits
            .iter()
            .map(|h| {
                Hit::new(
                    h["file_path"].as_str().unwrap_or("").to_owned(),
                    h["text"].as_str().unwrap_or("").to_owned(),
                    h["score"].as_f64().unwrap_or(f64::NAN) as f32,
                    h["heading_path"].as_str().map(str::to_owned),
                )
            })
            .collect())
    }
}
