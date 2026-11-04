//! Maieutik-Laya MCP Server
//!
//! Exposes Laya System 1 primitives and Maieutik retrieval via MCP.

use laya_primitives::{ChoiceOutput, LayaConfig, LayaSystem, NoulOutput, ScoreOutput};
use rmcp::{
    handler::server::ServerHandler,
    model::{CallToolRequestMethod, CallToolResult, Content, ServerCapabilities, Tool},
    service::RequestContext,
    transport::stdio,
    ErrorData, RoleServer, ServiceExt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[allow(dead_code)]
struct ChoiceRequest {
    context: String,
    options: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[allow(dead_code)]
struct ScoreRequest {
    query: String,
    chunks: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[allow(dead_code)]
struct NoulRequest {
    text: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct HealthResponse {
    status: String,
    encoder_loaded: bool,
    noul_loaded: bool,
    choice_loaded: bool,
    score_loaded: bool,
}

#[derive(Clone)]
struct MaieutikLayaServer {
    system: Arc<tokio::sync::RwLock<Option<LayaSystem>>>,
}

impl MaieutikLayaServer {
    fn new() -> Self {
        Self {
            system: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    async fn ensure_initialized(&self) -> Result<(), String> {
        let mut system_guard = self.system.write().await;
        if system_guard.is_none() {
            info!("Initializing LayaSystem...");
            let config = LayaConfig::default();
            let system = LayaSystem::load(config).await.map_err(|e| e.to_string())?;
            *system_guard = Some(system);
            info!("LayaSystem initialized successfully");
        }
        Ok(())
    }

    fn make_tools() -> Vec<Tool> {
        let make_tool = |name: String, description: String, schema: serde_json::Value| Tool {
            name: name.into(),
            description: Some(description.into()),
            input_schema: Arc::new(serde_json::from_value(schema).unwrap()),
            annotations: None,
        };

        vec![
            make_tool(
                "health".into(),
                "Health check - returns system status".into(),
                json!({"type": "object", "properties": {}}),
            ),
            make_tool(
                "noul".into(),
                "Noul guardrail - detects injection, OOD, null inputs".into(),
                json!({
                    "type": "object",
                    "properties": {
                        "text": {"type": "string", "description": "Text to analyze"}
                    },
                    "required": ["text"]
                }),
            ),
            make_tool(
                "choice".into(),
                "Choice router - selects best option from closed set".into(),
                json!({
                    "type": "object",
                    "properties": {
                        "context": {"type": "string", "description": "Context for routing"},
                        "options": {"type": "array", "items": {"type": "string"}, "description": "Options to choose from"}
                    },
                    "required": ["context", "options"]
                }),
            ),
            make_tool(
                "score".into(),
                "Score critic - evaluates chunk relevance for RAG".into(),
                json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Query text"},
                        "chunks": {"type": "array", "items": {"type": "string"}, "description": "Chunks to score"}
                    },
                    "required": ["query", "chunks"]
                }),
            ),
        ]
    }

    #[allow(clippy::unnecessary_lazy_evaluations)]
    #[allow(clippy::explicit_auto_deref)]
    #[allow(clippy::needless_borrow)]
    async fn handle_call(
        &self,
        name: &str,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult, ErrorData> {
        let args_map = args.as_ref().cloned().unwrap_or_default();
        match name {
            "health" => {
                // S'assurer que le système est initialisé
                self.ensure_initialized()
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

                let system_guard = self.system.read().await;
                let system = system_guard
                    .as_ref()
                    .ok_or_else(|| ErrorData::internal_error("System not initialized", None))?;

                let noul_loaded = async {
                    let guard = system.noul.read().await;
                    (&*guard).is_loaded()
                }
                .await;

                let choice_loaded = async {
                    let guard = system.choice.read().await;
                    (&*guard).is_loaded()
                }
                .await;

                let score_loaded = async {
                    let guard = system.score.read().await;
                    (&*guard).is_loaded()
                }
                .await;

                let response = HealthResponse {
                    status: "healthy".to_string(),
                    encoder_loaded: true,
                    noul_loaded,
                    choice_loaded,
                    score_loaded,
                };

                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response).unwrap(),
                )]))
            }
            "noul" => {
                self.ensure_initialized()
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                let system = self.system.read().await;
                let system = system
                    .as_ref()
                    .ok_or_else(|| ErrorData::internal_error("System not initialized", None))?;

                let text = args_map.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let output: NoulOutput = system
                    .noul(text)
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&output).unwrap(),
                )]))
            }
            "choice" => {
                self.ensure_initialized()
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                let system = self.system.read().await;
                let system = system
                    .as_ref()
                    .ok_or_else(|| ErrorData::internal_error("System not initialized", None))?;

                let context = args_map
                    .get("context")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let options = args_map
                    .get("options")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        let vec: Vec<_> = arr
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                        vec
                    })
                    .unwrap_or_default();

                let output: ChoiceOutput = system
                    .choice(context, &options)
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&output).unwrap(),
                )]))
            }
            "score" => {
                self.ensure_initialized()
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                let system = self.system.read().await;
                let system = system
                    .as_ref()
                    .ok_or_else(|| ErrorData::internal_error("System not initialized", None))?;

                let query = args_map.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let chunks = args_map
                    .get("chunks")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        let vec: Vec<_> = arr
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                        vec
                    })
                    .unwrap_or_default();

                let output: ScoreOutput = system
                    .score(query, &chunks)
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&output).unwrap(),
                )]))
            }
            _ => Err(ErrorData::method_not_found::<CallToolRequestMethod>()),
        }
    }
}

impl ServerHandler for MaieutikLayaServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::default(),
            capabilities: ServerCapabilities {
                tools: Some(rmcp::model::ToolsCapability {
                    list_changed: Some(false),
                }),
                ..Default::default()
            },
            server_info: rmcp::model::Implementation {
                name: "maieutik-laya".to_string(),
                version: "0.1.0".to_string(),
            },
            instructions: None,
        }
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, ErrorData> {
        Ok(rmcp::model::ListToolsResult {
            tools: Self::make_tools(),
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.handle_call(&request.name, request.arguments).await
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // stdout est le canal MCP : toute ligne de log y casserait le protocole.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    info!("Starting Maieutik-Laya MCP Server...");

    let server = MaieutikLayaServer::new();
    let transport = stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;

    Ok(())
}
