//! Local JSON-RPC server (Unix socket) exposing the application API.
//!
//! Transport: Unix domain socket (Linux) / stdio / TCP
//! Payload: 4-byte little-endian length prefix + UTF-8 JSON.

use anyhow::Result;
use jsonrpsee::core::{async_trait, RpcResult};
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::server::ServerBuilder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

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
pub struct McpServer {
    settings: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    // TODO: add file index, vector index, LLM, etc.
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            settings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start server over stdio (for MCP clients like Claude Desktop)
    pub async fn run_stdio(self: Arc<Self>) -> Result<()> {
        info!("Starting MCP server over stdio");
        // Server setup pending — MCP transport not yet implemented
        Ok(())
    }

    /// Start server over Unix domain socket (Linux)
    pub async fn run_unix_socket(self: Arc<Self>, socket_path: &str) -> Result<()> {
        info!("Starting MCP server on Unix socket: {}", socket_path);
        // Clean up any existing socket
        let _ = std::fs::remove_file(socket_path);

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

    async fn handle_unix_connection(self: Arc<Self>, mut stream: tokio::net::UnixStream) -> Result<()> {
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

    async fn dispatch(&self, request: RpcRequest) -> RpcResponse {
        // TODO: implement actual method dispatch
        RpcResponse {
            ok: false,
            result: None,
            error: Some("Not implemented".to_string()),
            id: request.id,
        }
    }
}

/// Length-prefixed JSON framing for the local socket protocol.
pub mod framed {
    use anyhow::Result;
    use serde::{Deserialize, Serialize};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;
    use tracing::debug;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RpcRequest {
        pub method: String,
        pub params: serde_json::Value,
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

    pub fn socket_path() -> String {
        format!("/tmp/maieutik_mcp_{}.sock", std::process::id())
    }

    pub const MAX_REQUEST_SIZE: usize = 1024 * 1024; // 1 MB

    pub async fn connect() -> Result<UnixStream> {
        UnixStream::connect(socket_path()).await.map_err(Into::into)
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