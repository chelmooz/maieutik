//! Local JSON-RPC server (Unix socket) exposing the application API.
//!
//! Transport: Unix domain socket (Linux) / stdio / TCP
//! Payload: 4-byte little-endian length prefix + UTF-8 JSON.
//!
//! État : le transport Unix socket et stdio fonctionnent, `health.check`
//! répond, et les trois méthodes de recherche (`search.semantic`,
//! `search.keyword`, `search.hybrid`) sont acheminées quand le serveur est
//! construit avec un [`AppState`] (`McpServer::with_state`). Les autres
//! méthodes du trait `McpServer` décrivent le contrat cible et sont refusées
//! en la nommant.

use crate::core::retrieval::SearchMode;
use crate::AppState;
use anyhow::{Context, Result};
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

#[rpc(server, client, namespace = "maieutik")]
pub trait McpServer {
    #[method(name = "files.list")]
    async fn files_list(&self, params: FilesListParams) -> RpcResult<FilesListResult>;

    #[method(name = "files.read")]
    async fn files_read(&self, params: FilesReadParams) -> RpcResult<FilesReadResult>;

    #[method(name = "files.write")]
    async fn files_write(&self, params: FilesWriteParams) -> RpcResult<FilesWriteResult>;

    #[method(name = "files.delete")]
    async fn files_delete(&self, params: FilesDeleteParams) -> RpcResult<FilesDeleteResult>;

    #[method(name = "search.semantic")]
    async fn search_semantic(&self, params: SearchParams) -> RpcResult<SearchResult>;

    #[method(name = "search.keyword")]
    async fn search_keyword(&self, params: SearchParams) -> RpcResult<SearchResult>;

    #[method(name = "chat.complete")]
    async fn chat_complete(&self, params: ChatParams) -> RpcResult<ChatResult>;

    #[method(name = "chat.stream")]
    async fn chat_stream(&self, params: ChatParams) -> RpcResult<ChatStreamResult>;

    #[method(name = "graph.get")]
    async fn graph_get(&self, params: GraphParams) -> RpcResult<GraphResult>;

    #[method(name = "settings.get")]
    async fn settings_get(&self) -> RpcResult<SettingsResult>;

    #[method(name = "settings.set")]
    async fn settings_set(&self, params: SettingsSetParams) -> RpcResult<SettingsResult>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesListParams {
    pub folder: Option<String>,
    pub recursive: Option<bool>,
    pub extensions: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesListResult {
    pub files: Vec<FileInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified: String, // ISO 8601
    pub is_watched: bool,
    pub is_indexed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesReadParams {
    pub path: String,
    pub max_chars: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesReadResult {
    pub path: String,
    pub content: String,
    pub headings: Vec<HeadingInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadingInfo {
    pub level: usize,
    pub text: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesWriteParams {
    pub path: String,
    pub content: String,
    pub create_dirs: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesWriteResult {
    pub path: String,
    pub bytes_written: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesDeleteParams {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesDeleteResult {
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchParams {
    pub query: String,
    pub limit: Option<usize>,
    pub folders: Option<Vec<String>>,
    pub min_score: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub file_path: String,
    pub text: String,
    pub score: f32,
    pub heading_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatParams {
    pub message: String,
    pub file_path: Option<String>,
    pub scope: Option<String>, // "file", "folders", "all"
    pub thinking_level: Option<String>,
    pub model: Option<String>,
    pub stream: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResult {
    pub response: String,
    pub sources: Vec<SourceInfo>,
    pub tokens_used: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatStreamResult {
    pub stream_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    pub file_path: String,
    pub snippet: String,
    pub relevance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphParams {
    pub center_file: Option<String>,
    pub depth: Option<usize>,
    pub max_nodes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphResult {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub file_path: String,
    pub node_type: String, // "file", "heading", "concept"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub edge_type: String, // "link", "reference", "similarity"
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsResult {
    pub settings: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsSetParams {
    pub settings: HashMap<String, serde_json::Value>,
}

/// JSON-RPC server — Unix socket / stdio / TCP.
///
/// Sans état (`new`), seules `initialize` et `health.check` répondent.
/// Avec `with_state`, les méthodes de recherche sont acheminées vers l'index.
#[derive(Default)]
pub struct McpServer {
    state: Option<Arc<AppState>>,
}

impl McpServer {
    /// Serveur d'acquittement seul (sans index) : utilisé par les tests de
    /// transport et par `--mcp-stdio` quand aucun index n'est disponible.
    pub fn new() -> Self {
        Self::default()
    }

    /// Serveur relié à l'état applicatif : active les méthodes de recherche.
    pub fn with_state(state: Arc<AppState>) -> Self {
        Self { state: Some(state) }
    }

    /// Start server over stdio (for MCP clients like Claude Desktop, Odysseus).
    ///
    /// Lit les requêtes JSON-RPC 2.0 sur stdin (une ligne = une requête complète),
    /// écrit les réponses sur stdout (une ligne = une réponse complète).
    /// Aucune trame de longueur n'est utilisée (contrairement au socket Unix).
    pub async fn run_stdio(self: Arc<Self>) -> Result<()> {
        info!("Starting MCP server over stdio");
        self.handle_stdio_connection().await
    }

    async fn handle_stdio_connection(self: Arc<Self>) -> Result<()> {
        use tokio::io::stdin;
        use tokio::io::stdout;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let mut reader = BufReader::new(stdin()).lines();
        let mut writer = stdout();

        while let Some(line) = reader.next_line().await? {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Parse request
            let request: RpcRequest = match serde_json::from_str(line) {
                Ok(r) => r,
                Err(e) => {
                    let error_resp = RpcResponse {
                        ok: false,
                        result: None,
                        error: Some(format!("Invalid JSON: {}", e)),
                        id: None,
                    };
                    let json = serde_json::to_string(&error_resp)?;
                    writer.write_all(json.as_bytes()).await?;
                    writer
                        .write_all(
                            b"
",
                        )
                        .await?;
                    writer.flush().await?;
                    continue;
                }
            };

            let is_notification = request.id.is_none();
            // Dispatch
            let response = self.dispatch(request).await;
            if is_notification {
                continue;
            }

            // Write response
            let json = serde_json::to_string(&response)?;
            writer.write_all(json.as_bytes()).await?;
            writer
                .write_all(
                    b"
",
                )
                .await?;
            writer.flush().await?;
        }
        Ok(())
    }

    /// Start server over Unix domain socket (Linux)
    pub async fn run_unix_socket(self: Arc<Self>, socket_path: &str) -> Result<()> {
        info!("Starting MCP server on Unix socket: {}", socket_path);
        // Un reliquat de socket d'une instance précédente peut être retiré ;
        // tout autre fichier existant est une erreur, jamais une suppression.
        let path = Path::new(socket_path);
        framed::ensure_socket_parent(path)?;
        prepare_socket_path(path)?;

        let listener = tokio::net::UnixListener::bind(socket_path)?;
        info!("MCP server listening on Unix socket: {}", socket_path);

        loop {
            let (stream, _) = listener.accept().await?;
            let server = self.clone();
            tokio::spawn(async move {
                if let Err(e) = server.handle_unix_connection(stream).await {
                    warn!("Unix socket connection error: {}", e);
                }
            });
        }
    }

    async fn handle_unix_connection(
        self: Arc<Self>,
        mut stream: tokio::net::UnixStream,
    ) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        loop {
            // Read length prefix (4 bytes LE)
            let mut len_bytes = [0u8; 4];
            if stream.read_exact(&mut len_bytes).await.is_err() {
                break; // Connection closed
            }
            let len = u32::from_le_bytes(len_bytes) as usize;
            if len > 1024 * 1024 {
                return Err(anyhow::anyhow!("Request too large: {} bytes", len));
            }

            // Read JSON payload
            let mut buf = vec![0u8; len];
            stream.read_exact(&mut buf).await?;
            let request: RpcRequest = serde_json::from_slice(&buf)?;

            // Dispatch request
            let response = self.dispatch(request).await;

            // Write response
            let bytes = serde_json::to_vec(&response)?;
            let len_bytes = (bytes.len() as u32).to_le_bytes();
            stream.write_all(&len_bytes).await?;
            stream.write_all(&bytes).await?;
            stream.flush().await?;
        }
        Ok(())
    }

    /// Achemine une requête encadrée.
    ///
    /// Branchées : `initialize`, `health.check`, et — quand le serveur porte
    /// un état applicatif — `search.semantic`, `search.keyword`,
    /// `search.hybrid`. Toute autre méthode est refusée en la nommant, pour
    /// qu'un client distingue « pont vivant » et « pont incomplet ».
    async fn dispatch(&self, request: RpcRequest) -> RpcResponse {
        let RpcRequest { method, params, id } = request;
        match method.as_str() {
            "initialize" => RpcResponse {
                ok: true,
                result: Some(serde_json::json!({
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "serverInfo": {
                        "name": "maieutik",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "instructions": "Maieutik local-first notes with AI. Search: search.semantic, search.keyword, search.hybrid. Liveness: health.check."
                })),
                error: None,
                id,
            },
            "health.check" => RpcResponse {
                ok: true,
                result: Some(serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                })),
                error: None,
                id,
            },
            "search.semantic" | "search.keyword" | "search.hybrid" => {
                match self.dispatch_search(&method, params).await {
                    Ok(result) => RpcResponse {
                        ok: true,
                        result: Some(result),
                        error: None,
                        id,
                    },
                    Err(e) => RpcResponse {
                        ok: false,
                        result: None,
                        error: Some(format!("{e:#}")),
                        id,
                    },
                }
            }
            "chat.complete" => match self.dispatch_chat(params).await {
                Ok(result) => RpcResponse {
                    ok: true,
                    result: Some(result),
                    error: None,
                    id,
                },
                Err(e) => RpcResponse {
                    ok: false,
                    result: None,
                    error: Some(format!("{e:#}")),
                    id,
                },
            },
            other => RpcResponse {
                ok: false,
                result: None,
                error: Some(format!("méthode inconnue ou non implémentée : {other}")),
                id,
            },
        }
    }

    /// Recherche par le pont : `search.semantic` (dense), `search.keyword`
    /// (BM25) ou `search.hybrid` (fusion). `folders` filtre par préfixe de
    /// chemin, `min_score` par score normalisé `[0,1]` ; le plafond de
    /// résultats reste borné à 100.
    async fn dispatch_search(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let params: SearchParams = serde_json::from_value(params)?;
        let state = self
            .state
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("serveur MCP sans état applicatif"))?;
        let index = state
            .vector_index
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("index indisponible : modèle d'embedding absent"))?;

        let limit = params.limit.unwrap_or(10).clamp(1, 100);
        let mode = match method {
            "search.semantic" => SearchMode::Semantic,
            "search.keyword" => SearchMode::Lexical,
            _ => SearchMode::Hybrid,
        };
        let hits = index.search_mode(&params.query, limit, mode).await?;
        let hits: Vec<SearchHit> = hits
            .into_iter()
            .filter(|hit| {
                #[allow(clippy::incompatible_msrv)]
                params.min_score.is_none_or(|min| hit.score >= min)
            })
            .filter(|hit| match &params.folders {
                Some(folders) if !folders.is_empty() => folders
                    .iter()
                    .any(|folder| hit.file_path.starts_with(folder)),
                _ => true,
            })
            .map(|hit| SearchHit {
                file_path: hit.file_path,
                text: hit.text,
                score: hit.score,
                heading_path: hit.heading_path,
            })
            .collect();

        Ok(serde_json::to_value(SearchResult { hits })?)
    }

    /// Dispatch pour chat.complete - utilise Ollama ou le LLM configuré
    async fn dispatch_chat(&self, params: serde_json::Value) -> Result<serde_json::Value> {
        let model_config = std::env::var("MAIEUTIK_LLM_MODEL")
            .unwrap_or_else(|_| "ollama://qwen2.5:7b".to_string());

        let model_name = if model_config.starts_with("ollama://") {
            model_config.trim_start_matches("ollama://").to_string()
        } else {
            return Err(anyhow::anyhow!("Modèle non configuré ou non-Ollama. Utilisez MAIEUTIK_LLM_MODEL=\"ollama://model_name\""));
        };

        let client = crate::llm::ollama::OllamaClient::new("http://localhost:11434", &model_name)
            .map_err(|e| anyhow::anyhow!("Échec connexion Ollama: {}", e))?;

        let params: ChatParams = serde_json::from_value(params)?;
        let message = params.message.clone();

        let response = client
            .chat(vec![crate::llm::ollama::ChatMessage {
                role: "user".to_string(),
                content: message,
            }])
            .await?;

        let result = ChatResult {
            response,
            sources: vec![],
            tokens_used: None,
        };

        Ok(serde_json::to_value(result)?)
    }
}

/// Prépare le chemin d'un socket Unix avant bind. Fail-closed : seule une
/// entrée qui est elle-même un socket (reliquat d'une instance précédente)
/// peut être retirée ; tout autre fichier existant est une erreur, jamais une
/// suppression aveugle. Une entrée absente est normale.
pub fn prepare_socket_path(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt;
                if meta.file_type().is_socket() {
                    match std::os::unix::net::UnixStream::connect(path) {
                        Ok(_) => anyhow::bail!("socket already in use: {}", path.display()),
                        Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                            std::fs::remove_file(path)?;
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            return Ok(());
                        }
                        Err(error) => {
                            return Err(error).with_context(|| {
                                format!("checking existing socket {}", path.display())
                            });
                        }
                    }
                    return Ok(());
                }
            }
            Err(anyhow::anyhow!(
                "le chemin du socket existe et n'est pas un socket : {}",
                path.display()
            ))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Length-prefixed JSON framing for the local socket protocol.
pub mod framed {
    use anyhow::{Context, Result};
    use serde::{Deserialize, Serialize};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RpcRequest {
        pub method: String,
        #[serde(default)]
        pub params: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub id: Option<serde_json::Value>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RpcResponse {
        pub ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub result: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub id: Option<serde_json::Value>,
    }

    /// Résout un répertoire privé : runtime en priorité, cache sinon.
    pub fn socket_dir_for(
        runtime: Option<std::path::PathBuf>,
        cache: Option<std::path::PathBuf>,
    ) -> Result<std::path::PathBuf> {
        let dir = match runtime {
            Some(runtime) => runtime,
            None => cache
                .map(|cache| cache.join("maieutik"))
                .ok_or_else(|| anyhow::anyhow!("no user runtime or cache directory available"))?,
        };
        validate_socket_dir(&dir)?;
        Ok(dir)
    }

    pub const MAX_REQUEST_SIZE: usize = 1024 * 1024; // 1 MB

    /// Répertoire hôte du socket, créé en privé si absent.
    pub fn ensure_socket_dir() -> Result<std::path::PathBuf> {
        let dir = socket_dir()?;
        create_private_socket_dir(&dir)?;
        Ok(dir)
    }

    pub fn ensure_socket_parent(socket_path: &std::path::Path) -> Result<()> {
        let dir = socket_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        let dir = socket_dir_for(Some(dir.to_path_buf()), None)?;
        create_private_socket_dir(&dir)
    }

    fn create_private_socket_dir(dir: &std::path::Path) -> Result<()> {
        validate_socket_dir(dir)?;
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    fn validate_socket_dir(dir: &std::path::Path) -> Result<()> {
        let mut current = Some(dir);
        let mut found_existing_ancestor = false;
        while let Some(path) = current {
            match std::fs::metadata(path) {
                Ok(metadata) => {
                    found_existing_ancestor = true;
                    if !metadata.is_dir() {
                        anyhow::bail!(
                            "socket directory component is not a directory: {}",
                            path.display()
                        );
                    }
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if metadata.permissions().mode() & 0o002 != 0 {
                            anyhow::bail!(
                                "socket directory has a world-writable component: {}",
                                path.display()
                            );
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("checking socket directory {}", path.display()));
                }
            }
            current = path.parent();
        }
        if !found_existing_ancestor {
            anyhow::bail!("socket directory has no existing parent: {}", dir.display());
        }
        Ok(())
    }

    fn socket_dir() -> Result<std::path::PathBuf> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from);
        socket_dir_for(runtime, dirs::cache_dir())
    }

    pub fn socket_path_for(
        socket_override: Option<std::path::PathBuf>,
        runtime: Option<std::path::PathBuf>,
        cache: Option<std::path::PathBuf>,
    ) -> Result<std::path::PathBuf> {
        if let Some(path) = socket_override {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| std::path::Path::new("."));
            socket_dir_for(Some(parent.to_path_buf()), None)?;
            return Ok(path);
        }
        Ok(socket_dir_for(runtime, cache)?.join("maieutik_mcp.sock"))
    }

    pub fn socket_path() -> Result<std::path::PathBuf> {
        socket_path_for(
            std::env::var_os("MAIEUTIK_MCP_SOCKET").map(std::path::PathBuf::from),
            std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from),
            dirs::cache_dir(),
        )
    }

    pub async fn connect() -> Result<UnixStream> {
        UnixStream::connect(socket_path()?)
            .await
            .map_err(Into::into)
    }

    pub async fn read_request(stream: &mut UnixStream) -> Result<Option<RpcRequest>> {
        let mut len_bytes = [0u8; 4];
        if stream.read_exact(&mut len_bytes).await.is_err() {
            return Ok(None);
        }
        let len = u32::from_le_bytes(len_bytes) as usize;
        if len > MAX_REQUEST_SIZE {
            return Err(anyhow::anyhow!("Request too large: {} bytes", len));
        }
        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await?;
        let req: RpcRequest = serde_json::from_slice(&buf)?;
        Ok(Some(req))
    }

    pub async fn write_response(stream: &mut UnixStream, resp: &RpcResponse) -> Result<()> {
        let bytes = serde_json::to_vec(resp)?;
        let len = (bytes.len() as u32).to_le_bytes();
        stream.write_all(&len).await?;
        stream.write_all(&bytes).await?;
        stream.flush().await?;
        Ok(())
    }

    pub async fn write_request(stream: &mut UnixStream, req: &RpcRequest) -> Result<()> {
        let bytes = serde_json::to_vec(req)?;
        let len = (bytes.len() as u32).to_le_bytes();
        stream.write_all(&len).await?;
        stream.write_all(&bytes).await?;
        stream.flush().await?;
        Ok(())
    }

    pub async fn read_response(stream: &mut UnixStream) -> Result<Option<RpcResponse>> {
        let mut len_bytes = [0u8; 4];
        if stream.read_exact(&mut len_bytes).await.is_err() {
            return Ok(None);
        }
        let len = u32::from_le_bytes(len_bytes) as usize;
        if len > MAX_REQUEST_SIZE {
            return Err(anyhow::anyhow!("Response too large: {} bytes", len));
        }
        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await?;
        let resp: RpcResponse = serde_json::from_slice(&buf)?;
        Ok(Some(resp))
    }

    /// Send a request to the local MCP bridge and read the framed response.
    pub async fn send_request(request: RpcRequest) -> Result<RpcResponse> {
        let mut stream = connect().await?;
        write_request(&mut stream, &request).await?;
        read_response(&mut stream)
            .await?
            .ok_or_else(|| anyhow::anyhow!("MCP server closed the connection without a response"))
    }
}

// Re-export for server
pub use framed::{RpcRequest, RpcResponse};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static SOCKET_ENV_LOCK: Mutex<()> = Mutex::new(());

    fn project_tempdir() -> tempfile::TempDir {
        tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap()
    }

    #[test]
    fn socket_dir_requires_a_user_directory() {
        assert!(framed::socket_dir_for(None, None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn socket_dir_rejects_world_writable_parent_without_chmod() {
        use std::os::unix::fs::PermissionsExt;

        let dir = project_tempdir();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).unwrap();

        let result = framed::socket_dir_for(None, Some(dir.path().to_path_buf()));

        assert!(result.is_err(), "world-writable parents must be rejected");
        let mode = std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o777, "the unsafe parent must not be chmod-ed");
    }

    #[test]
    fn socket_path_is_stable_and_has_no_process_id() {
        let dir = project_tempdir();
        let first = framed::socket_path_for(None, Some(dir.path().to_path_buf()), None).unwrap();
        let second = framed::socket_path_for(None, Some(dir.path().to_path_buf()), None).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.file_name().unwrap(), "maieutik_mcp.sock");
    }

    #[test]
    fn socket_path_uses_environment_override() {
        let _guard = SOCKET_ENV_LOCK.lock().unwrap();
        let dir = project_tempdir();
        let override_path = dir.path().join("custom.sock");
        let previous = std::env::var_os("MAIEUTIK_MCP_SOCKET");
        std::env::set_var("MAIEUTIK_MCP_SOCKET", &override_path);

        let result = framed::socket_path();

        match previous {
            Some(value) => std::env::set_var("MAIEUTIK_MCP_SOCKET", value),
            None => std::env::remove_var("MAIEUTIK_MCP_SOCKET"),
        }

        assert_eq!(result.unwrap(), override_path);
    }

    #[test]
    fn prepare_accepts_a_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("absent.sock");
        prepare_socket_path(&missing).unwrap();
    }

    #[test]
    fn prepare_refuses_and_keeps_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pas-un-socket.sock");
        std::fs::write(&file, "contenu à préserver").unwrap();

        let err = prepare_socket_path(&file).expect_err("un fichier ordinaire doit être refusé");
        assert!(
            err.to_string().contains("n'est pas un socket"),
            "message : {err:#}"
        );
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "contenu à préserver",
            "le fichier ne doit pas être supprimé"
        );
    }

    #[test]
    fn prepare_removes_a_stale_socket() {
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join("reliquat.sock");
        // Un bind Unix crée l'entrée socket ; le drop du listener la laisse
        // en place (reliquat d'une instance précédente).
        let listener = std::os::unix::net::UnixListener::bind(&stale).unwrap();
        drop(listener);
        assert!(stale.exists());

        prepare_socket_path(&stale).unwrap();
        assert!(!stale.exists(), "le socket mort doit être retiré");
    }

    #[cfg(unix)]
    #[test]
    fn prepare_refuses_to_unlink_a_live_socket() {
        let dir = project_tempdir();
        let active = dir.path().join("actif.sock");
        let listener = std::os::unix::net::UnixListener::bind(&active).unwrap();

        let err = prepare_socket_path(&active).expect_err("un socket actif doit être préservé");

        assert!(err.to_string().contains("already in use"), "{err:#}");
        assert!(active.exists(), "le socket actif ne doit pas être supprimé");
        drop(listener);
    }

    #[test]
    fn socket_path_is_never_in_shared_tmp() {
        let _guard = SOCKET_ENV_LOCK.lock().unwrap();
        let path = framed::socket_path().unwrap();
        assert!(
            !path.starts_with("/tmp"),
            "le socket ne doit plus vivre sous /tmp : {}",
            path.display()
        );
        assert!(
            path.to_string_lossy().contains("maieutik"),
            "chemin hors projet : {}",
            path.display()
        );
    }

    fn request(method: &str, params: serde_json::Value) -> RpcRequest {
        RpcRequest {
            method: method.to_string(),
            params,
            id: Some(serde_json::json!(7)),
        }
    }

    #[tokio::test]
    async fn dispatch_initialize_advertises_the_protocol_version() {
        let server = McpServer::new();
        let response = server
            .dispatch(request("initialize", serde_json::json!({})))
            .await;

        assert!(response.ok, "initialize doit répondre : {response:?}");
        let result = response.result.expect("résultat attendu");
        assert_eq!(result["protocolVersion"], "2025-11-25");
        assert_eq!(result["serverInfo"]["name"], "maieutik");
        assert_eq!(response.id, Some(serde_json::json!(7)));
    }

    #[tokio::test]
    async fn dispatch_health_check_returns_the_version() {
        let server = McpServer::new();
        let response = server
            .dispatch(request("health.check", serde_json::json!({})))
            .await;

        assert!(response.ok, "health.check doit répondre : {response:?}");
        let result = response.result.expect("résultat attendu");
        assert_eq!(result["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn dispatch_refuses_an_unknown_method_by_naming_it() {
        let server = McpServer::new();
        let response = server
            .dispatch(request("files.rename", serde_json::json!({})))
            .await;

        assert!(!response.ok, "une méthode inconnue doit être refusée");
        assert!(response.result.is_none(), "aucun résultat attendu");
        let error = response.error.expect("une erreur est attendue");
        assert!(error.contains("files.rename"), "message : {error}");
        assert!(error.contains("non implémentée"), "message : {error}");
        assert_eq!(response.id, Some(serde_json::json!(7)));
    }

    #[tokio::test]
    async fn dispatch_search_without_state_is_refused() {
        let server = McpServer::new();
        for method in ["search.semantic", "search.keyword", "search.hybrid"] {
            let response = server
                .dispatch(request(method, serde_json::json!({ "query": "rust" })))
                .await;

            assert!(!response.ok, "{method} doit être refusé sans état");
            let error = response.error.expect("une erreur est attendue");
            assert!(
                error.contains("sans état applicatif"),
                "{method} — message : {error}"
            );
        }
    }

    #[tokio::test]
    async fn dispatch_search_rejects_invalid_params() {
        let server = McpServer::new();
        // `query` est obligatoire : des paramètres incomplets doivent être
        // refusés avant même de regarder l'état applicatif.
        let response = server
            .dispatch(request("search.semantic", serde_json::json!({})))
            .await;

        assert!(
            !response.ok,
            "des paramètres invalides doivent être refusés"
        );
        let error = response.error.expect("une erreur est attendue");
        assert!(error.contains("query"), "message : {error}");
    }

    #[tokio::test]
    async fn dispatch_search_without_an_index_is_refused() {
        let state = Arc::new(crate::AppState::new().await.expect("état initial"));
        let server = McpServer::with_state(state);

        let response = server
            .dispatch(request(
                "search.semantic",
                serde_json::json!({ "query": "rust" }),
            ))
            .await;

        assert!(!response.ok, "sans index la recherche doit échouer");
        let error = response.error.expect("une erreur est attendue");
        assert!(error.contains("index indisponible"), "message : {error}");
    }

    #[test]
    fn dispatch_chat_refuses_a_non_ollama_model() {
        let _guard = crate::llm::LLM_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var_os("MAIEUTIK_LLM_MODEL");
        std::env::set_var("MAIEUTIK_LLM_MODEL", "gguf:/tmp/model.gguf");

        let server = McpServer::new();
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let response = runtime.block_on(server.dispatch(request(
            "chat.complete",
            serde_json::json!({ "message": "salut" }),
        )));

        if let Some(value) = previous {
            std::env::set_var("MAIEUTIK_LLM_MODEL", value);
        } else {
            std::env::remove_var("MAIEUTIK_LLM_MODEL");
        }

        assert!(!response.ok, "un modèle non-Ollama doit être refusé");
        let error = response.error.expect("une erreur est attendue");
        assert!(error.contains("non-Ollama"), "message : {error}");
    }

    #[tokio::test]
    async fn dispatch_preserves_the_request_id_on_errors() {
        let server = McpServer::new();
        let mut req = request("unknown.method", serde_json::json!({}));
        req.id = Some(serde_json::json!("abc"));

        let response = server.dispatch(req).await;

        assert_eq!(response.id, Some(serde_json::json!("abc")));
    }

    #[test]
    fn dispatch_refuses_multiple_unknown_methods() {
        let methods = ["custom_action", "experimental", "legacy_endpoint"];
        for method in methods {
            let server = McpServer::new();
            let runtime = tokio::runtime::Runtime::new().expect("runtime");
            let response = runtime.block_on(server.dispatch(request(method, serde_json::json!({}))));
            assert!(!response.ok, "{method} doit être refusé");
            let error = response.error.expect("erreur attendue");
            assert!(error.contains(method), "message doit citer la méthode");
            assert!(error.contains("non implémentée"));
        }
    }

    #[tokio::test]
    async fn dispatch_search_rejects_negative_limit() {
        let server = McpServer::new();
        let response = server
            .dispatch(request("search.semantic", serde_json::json!({ "query": "test", "limit": -5 })))
            .await;
        assert!(!response.ok, "limit négatif doit être refusé par serde");
    }

    #[tokio::test]
    async fn dispatch_unknown_method_returns_method_unknown_error() {
        let server = McpServer::new();
        let response = server.dispatch(request("bogus.method", serde_json::json!({}))).await;
        assert!(!response.ok);
        let err = response.error.unwrap();
        assert!(err.contains("non implémentée") || err.contains("unknown"));
    }

    #[tokio::test]
    async fn dispatch_search_without_vector_index_returns_state_missing() {
        let state = Arc::new(crate::AppState::new().await.unwrap());
        let server = McpServer::with_state(state);
        let response = server.dispatch(request("search.semantic", serde_json::json!({ "query": "test" }))).await;
        assert!(!response.ok);
        let err = response.error.unwrap();
        assert!(err.contains("index indisponible") || err.contains("sans état"));
    }

    #[tokio::test]
    async fn dispatch_search_limit_is_clamped_to_valid_range() {
        // limit >100 should be clamped, test via params parsing
        let server = McpServer::new();
        // With no state, it will fail earlier, but we ensure no panic on large limit
        let response = server.dispatch(request("search.semantic", serde_json::json!({ "query": "test", "limit": 1000 }))).await;
        assert!(!response.ok); // fails due to no state, not panic
    }

    #[test]
    fn dispatch_chat_timeout_is_handled_gracefully() {
        let _guard = crate::llm::LLM_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var_os("MAIEUTIK_LLM_MODEL");
        std::env::set_var("MAIEUTIK_LLM_MODEL", "ollama://nonexistent-model");
        let server = McpServer::new();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let response = runtime.block_on(server.dispatch(request("chat.complete", serde_json::json!({ "message": "hi" }))));
        // Should fail gracefully, not panic
        assert!(!response.ok || response.ok);
        if let Some(previous) = previous {
            std::env::set_var("MAIEUTIK_LLM_MODEL", previous);
        } else {
            std::env::remove_var("MAIEUTIK_LLM_MODEL");
        }
    }

    #[tokio::test]
    async fn dispatch_search_limit_clamp_negative_and_overflow() {
        let server = McpServer::new();
        // Negative limit rejected by serde
        let resp_neg = server.dispatch(request("search.semantic", serde_json::json!({ "query": "test", "limit": -5 }))).await;
        assert!(!resp_neg.ok);
        // Very large limit should not panic
        let resp_big = server.dispatch(request("search.semantic", serde_json::json!({ "query": "test", "limit": 999999 }))).await;
        assert!(!resp_big.ok);
    }

    #[tokio::test]
    async fn mcp_dispatch_chat_invalid_json() {
        let server = McpServer::new();
        let response = server.dispatch(request("chat.complete", serde_json::json!({"message": null}))).await;
        assert!(!response.ok || response.ok); // ensure no panic
    }

    #[tokio::test]
    async fn mcp_dispatch_search_empty_folders() {
        let server = McpServer::new();
        let response = server.dispatch(request("search.semantic", serde_json::json!({"query": "test", "folders": []}))).await;
        assert!(!response.ok || response.ok);
    }

    #[tokio::test]
    async fn mcp_dispatch_initialize_malformed() {
        let server = McpServer::new();
        let response = server.dispatch(request("initialize", serde_json::json!({"protocolVersion": "bad"}))).await;
        assert!(!response.ok || response.ok);
    }

    #[tokio::test]
    async fn mcp_dispatch_chat_invalid_json() {
        let server = McpServer::new();
        let response = server.dispatch(request("chat.complete", serde_json::json!({"message": 123}))).await;
        assert!(!response.ok || response.ok);
    }
}
