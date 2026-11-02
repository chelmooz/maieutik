//! Adaptateur Laya : System 1 via le serveur MCP `maieutik-mcp` (stdio).

use crate::mcp_client::{Launch, McpClient};
use crate::ports::*;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::time::Duration;

pub struct LayaMcp {
    client: McpClient,
    /// Vrai si l'UI a lancé le serveur avec `LAYA_MOCK=1` (le serveur ne le dit pas).
    pub mock: bool,
}

impl LayaMcp {
    /// `LAYA_MOCK` est posé explicitement : sans lui, `LayaConfig::default()` choisit le mock.
    pub fn connect(mut launch: Launch, mock: bool, timeout: Duration) -> Result<Self, PortError> {
        launch.env.push(("LAYA_MOCK".into(), if mock { "1" } else { "0" }.into()));
        let client = McpClient::connect(&launch, timeout)?;
        Ok(Self { client, mock })
    }

    pub fn tools(&mut self) -> Result<Vec<String>, PortError> {
        self.client.list_tools()
    }

    pub fn health(&mut self) -> Result<LayaHealth, PortError> {
        self.call("health", json!({}))
    }

    pub fn noise_lines(&self) -> usize {
        self.client.noise_lines()
    }

    pub fn first_noise(&self) -> Option<&str> {
        self.client.first_noise()
    }

    pub fn server_label(&self) -> String {
        format!("{} {}", self.client.server_name, self.client.server_version)
    }

    fn call<T: DeserializeOwned>(&mut self, tool: &str, args: Value) -> Result<T, PortError> {
        let v = self.client.call_tool(tool, args)?;
        serde_json::from_value(v).map_err(|e| PortError::Protocol(format!("sortie {tool} : {e}")))
    }
}

impl System1Port for LayaMcp {
    fn is_mock(&self) -> bool {
        self.mock
    }

    fn noul(&mut self, text: &str) -> Result<NoulVerdict, PortError> {
        self.call("noul", json!({"text": text}))
    }

    fn choice(&mut self, context: &str, options: &[String]) -> Result<ChoiceVerdict, PortError> {
        self.call("choice", json!({"context": context, "options": options}))
    }

    fn score(&mut self, query: &str, chunks: &[String]) -> Result<ScoreVerdict, PortError> {
        self.call("score", json!({"query": query, "chunks": chunks}))
    }
}
