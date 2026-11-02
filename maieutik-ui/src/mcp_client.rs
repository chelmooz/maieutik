//! Client MCP minimal sur stdio (JSON-RPC 2.0, une ligne = un message).
//! Les lignes stdout non-JSON sont comptées (`noise_lines`) au lieu de casser
//! la session : `laya-primitives` logge sur stdout via `tracing_subscriber::fmt::init()`.

use crate::ports::PortError;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

enum Line {
    Json(Value),
    Noise(String),
    Eof,
}

#[derive(Debug, Clone, Default)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub workdir: Option<PathBuf>,
}

pub struct McpClient {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Line>,
    next_id: u64,
    timeout: Duration,
    noise: Vec<String>,
    pub server_name: String,
    pub server_version: String,
}

impl McpClient {
    pub fn connect(launch: &Launch, timeout: Duration) -> Result<Self, PortError> {
        let mut cmd = Command::new(&launch.program);
        cmd.args(&launch.args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        for (k, v) in &launch.env {
            cmd.env(k, v);
        }
        if let Some(dir) = &launch.workdir {
            cmd.current_dir(dir);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| PortError::Unavailable(format!("lancement {} : {e}", launch.program.display())))?;
        let stdin = child.stdin.take().ok_or_else(|| PortError::Io("stdin fils".into()))?;
        let stdout = child.stdout.take().ok_or_else(|| PortError::Io("stdout fils".into()))?;

        let (tx, rx) = channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let msg = match serde_json::from_str::<Value>(line.trim()) {
                    Ok(v) if v.is_object() => Line::Json(v),
                    _ => Line::Noise(line),
                };
                if tx.send(msg).is_err() {
                    return;
                }
            }
            let _ = tx.send(Line::Eof);
        });

        let mut client = Self {
            child,
            stdin,
            rx,
            next_id: 1,
            timeout,
            noise: Vec::new(),
            server_name: String::new(),
            server_version: String::new(),
        };
        client.handshake()?;
        Ok(client)
    }

    fn handshake(&mut self) -> Result<(), PortError> {
        let info = self.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "maieutik-ui", "version": env!("CARGO_PKG_VERSION")}
            }),
        )?;
        self.server_name = info["serverInfo"]["name"].as_str().unwrap_or("?").to_owned();
        self.server_version = info["serverInfo"]["version"].as_str().unwrap_or("?").to_owned();
        self.notify("notifications/initialized", json!({}))
    }

    pub fn list_tools(&mut self) -> Result<Vec<String>, PortError> {
        let res = self.request("tools/list", json!({}))?;
        Ok(res["tools"]
            .as_array()
            .map(|a| a.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
            .unwrap_or_default())
    }

    /// Appelle un outil ; renvoie le JSON contenu dans `content[0].text`.
    pub fn call_tool(&mut self, name: &str, args: Value) -> Result<Value, PortError> {
        let res = self.request("tools/call", json!({"name": name, "arguments": args}))?;
        let text = res["content"][0]["text"]
            .as_str()
            .ok_or_else(|| PortError::Protocol("content[0].text absent".into()))?;
        if res["isError"].as_bool().unwrap_or(false) {
            return Err(PortError::Refused(text.to_owned()));
        }
        serde_json::from_str(text).map_err(|e| PortError::Protocol(format!("JSON de l'outil : {e}")))
    }

    pub fn noise_lines(&self) -> usize {
        self.noise.len()
    }

    pub fn first_noise(&self) -> Option<&str> {
        self.noise.first().map(String::as_str)
    }

    fn send(&mut self, msg: &Value) -> Result<(), PortError> {
        let mut line = serde_json::to_vec(msg).map_err(|e| PortError::Io(e.to_string()))?;
        line.push(b'\n');
        self.stdin
            .write_all(&line)
            .and_then(|_| self.stdin.flush())
            .map_err(|e| PortError::Unavailable(format!("écriture vers le fils : {e}")))
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), PortError> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, PortError> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let deadline = Instant::now() + self.timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(Line::Json(v)) => {
                    if v["id"] != json!(id) {
                        continue;
                    }
                    if let Some(err) = v.get("error") {
                        let msg = err["message"].as_str().unwrap_or("erreur JSON-RPC");
                        return Err(PortError::Refused(msg.to_owned()));
                    }
                    return Ok(v["result"].clone());
                }
                Ok(Line::Noise(s)) => self.noise.push(s),
                Ok(Line::Eof) | Err(RecvTimeoutError::Disconnected) => {
                    return Err(PortError::Unavailable("le serveur MCP a fermé stdout".into()))
                }
                Err(RecvTimeoutError::Timeout) => return Err(PortError::Timeout(self.timeout.as_secs())),
            }
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}
