//! Odysseus (Python, hors dépôt) : sonde de présence uniquement. Odysseus est
//! client MCP (il appelle Maieutik/Laya) ; aucun contrat UI→Odysseus n'existe.

use crate::ports::*;
use std::path::PathBuf;

pub struct OdysseusProbe {
    pub root: PathBuf,
}

impl OdysseusProbe {
    pub const RAG_SERVER: &'static str = "mcp_servers/rag_server.py";

    pub fn inspect(&self) -> Result<String, PortError> {
        if !self.root.is_dir() {
            return Err(PortError::Unavailable(format!(
                "racine absente : {}",
                self.root.display()
            )));
        }
        let rag = self.root.join(Self::RAG_SERVER);
        if !rag.is_file() {
            return Err(PortError::Unavailable(format!(
                "{} introuvable",
                rag.display()
            )));
        }
        Ok(format!("{} présent", Self::RAG_SERVER))
    }
}

impl AgentPort for OdysseusProbe {
    fn submit(&mut self, _task: &str) -> Result<String, PortError> {
        Err(PortError::NotWired(
            "aucun contrat UI→Odysseus défini (Odysseus appelle Maieutik via MCP, pas l'inverse)"
                .into(),
        ))
    }
}
