//! État honnête des briques : l'UI ne montre jamais « vert » sans preuve.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Brick {
    Maieutik,
    LayaNoul,
    LayaChoice,
    LayaScore,
    Odysseus,
    Llm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    Docker1,
    Docker2,
    HostApp,
}

impl Brick {
    pub fn label(self) -> &'static str {
        match self {
            Brick::Maieutik => "Maieutik (RAG)",
            Brick::LayaNoul => "Laya · Noul (garde-fou)",
            Brick::LayaChoice => "Laya · Choice (routeur)",
            Brick::LayaScore => "Laya · Score (critique)",
            Brick::Odysseus => "Odysseus (agents)",
            Brick::Llm => "LLM System 2",
        }
    }

    /// Placement selon la matrice d'allocation du schéma Omarchy.
    pub fn host(self) -> Host {
        match self {
            Brick::LayaNoul | Brick::LayaChoice | Brick::LayaScore | Brick::Odysseus => {
                Host::Docker1
            }
            Brick::Llm => Host::Docker2,
            Brick::Maieutik => Host::HostApp,
        }
    }
}

impl Host {
    pub fn label(self) -> &'static str {
        match self {
            Host::Docker1 => "Docker #1 — décision rapide + pont MCP (CPU)",
            Host::Docker2 => "Docker #2 — calcul lourd (GPU)",
            Host::HostApp => "Hôte Omarchy — application",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BrickState {
    Up(String),
    Mock(String),
    Unavailable(String),
    NotWired(String),
}

impl BrickState {
    pub fn tag(&self) -> &'static str {
        match self {
            BrickState::Up(_) => "UP",
            BrickState::Mock(_) => "MOCK",
            BrickState::Unavailable(_) => "INDISPONIBLE",
            BrickState::NotWired(_) => "NON CÂBLÉ",
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            BrickState::Up(s)
            | BrickState::Mock(s)
            | BrickState::Unavailable(s)
            | BrickState::NotWired(s) => s,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BrickReport {
    pub brick: Brick,
    pub state: BrickState,
    pub latency_ms: Option<u64>,
}
