//! Worker unique : possède les adaptateurs (fils Laya, socket Maieutik) et parle
//! à l'UI par canaux. L'UI ne bloque jamais sur une E/S.

use crate::daemon::DaemonClient;
use crate::egress::{EgressError, EgressPolicy};
use crate::laya::LayaMcp;
use crate::mcp_client::Launch;
use crate::mock::{MockSearch, MockSystem1};
use crate::odysseus::OdysseusProbe;
use crate::pipeline::{Pipeline, PipelineConfig, Trace};
use crate::ports::*;
use crate::status::{Brick, BrickReport, BrickState};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct LayaSetup {
    pub launch: Launch,
    /// Mode simulé demandé au serveur (`LAYA_MOCK=1`).
    pub mock: bool,
}

#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub laya: Option<LayaSetup>,
    pub daemon_socket: Option<PathBuf>,
    pub odysseus_root: Option<PathBuf>,
    /// Endpoint du LLM System 2 (ex. Ollama) — doit être local.
    pub llm_endpoint: Option<String>,
    pub demo: bool,
    pub pipeline: PipelineConfig,
    pub timeout: Duration,
    pub egress: EgressPolicy,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            laya: None,
            daemon_socket: crate::daemon::default_socket_path(),
            odysseus_root: None,
            llm_endpoint: None,
            demo: false,
            pipeline: PipelineConfig::default(),
            timeout: Duration::from_secs(30),
            egress: EgressPolicy::default(),
        }
    }
}

impl BackendConfig {
    pub fn validate(&self) -> Result<(), EgressError> {
        if let Some(url) = &self.llm_endpoint {
            self.egress.check_url(url)?;
        }
        Ok(())
    }
}

pub enum Command {
    Probe,
    Run(String),
    SetPipeline(PipelineConfig),
    Quit,
}

pub enum Event {
    Health(Vec<BrickReport>),
    Trace(Box<Trace>),
}

pub struct Backend {
    tx: Sender<Command>,
    pub rx: Receiver<Event>,
    handle: Option<JoinHandle<()>>,
}

impl Backend {
    /// `wake` réveille l'UI (ex. `ctx.request_repaint()`) à chaque événement.
    pub fn start(
        cfg: BackendConfig,
        wake: impl Fn() + Send + 'static,
    ) -> Result<Self, EgressError> {
        cfg.validate()?;
        let (tx, cmd_rx) = channel::<Command>();
        let (ev_tx, rx) = channel::<Event>();
        let handle = thread::Builder::new()
            .name("maieutik-backend".into())
            .spawn(move || Worker::new(cfg).run(cmd_rx, ev_tx, wake))
            .expect("thread backend");
        Ok(Self {
            tx,
            rx,
            handle: Some(handle),
        })
    }

    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Quit);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

struct Worker {
    cfg: BackendConfig,
    laya: Option<LayaMcp>,
    laya_err: Option<PortError>,
}

impl Worker {
    fn new(cfg: BackendConfig) -> Self {
        Self {
            cfg,
            laya: None,
            laya_err: None,
        }
    }

    fn run(mut self, rx: Receiver<Command>, tx: Sender<Event>, wake: impl Fn()) {
        while let Ok(cmd) = rx.recv() {
            match cmd {
                Command::Quit => break,
                Command::SetPipeline(p) => self.cfg.pipeline = p,
                Command::Probe => {
                    let _ = tx.send(Event::Health(self.probe()));
                }
                Command::Run(prompt) => {
                    let _ = tx.send(Event::Trace(Box::new(self.run_pipeline(&prompt))));
                }
            }
            wake();
        }
    }

    fn ensure_laya(&mut self) {
        if self.laya.is_some() || self.cfg.demo {
            return;
        }
        let Some(setup) = self.cfg.laya.clone() else {
            self.laya_err = Some(PortError::Unavailable("Laya non configuré".into()));
            return;
        };
        match LayaMcp::connect(setup.launch, setup.mock, self.cfg.timeout) {
            Ok(l) => {
                self.laya = Some(l);
                self.laya_err = None;
            }
            Err(e) => self.laya_err = Some(e),
        }
    }

    fn probe(&mut self) -> Vec<BrickReport> {
        // Une sonde retente toujours la connexion (fils mort, binaire rebuildé…).
        if !self.cfg.demo {
            self.laya = None;
        }
        self.ensure_laya();
        let mut out = vec![self.probe_maieutik()];
        out.extend(self.probe_laya());
        out.push(self.probe_odysseus());
        out.push(BrickReport {
            brick: Brick::Llm,
            state: BrickState::NotWired(
                "génération jamais exécutée (VERIFICATION.md) — aucun adaptateur".into(),
            ),
            latency_ms: None,
        });
        out
    }

    fn probe_maieutik(&mut self) -> BrickReport {
        let brick = Brick::Maieutik;
        if self.cfg.demo {
            return BrickReport {
                brick,
                state: BrickState::Mock("corpus jouet intégré".into()),
                latency_ms: None,
            };
        }
        let Some(path) = self.cfg.daemon_socket.clone() else {
            let s = BrickState::Unavailable(
                "aucun chemin de socket (XDG_RUNTIME_DIR / MAIEUTIK_MCP_SOCKET)".into(),
            );
            return BrickReport {
                brick,
                state: s,
                latency_ms: None,
            };
        };
        let mut c = DaemonClient::new(path, Duration::from_secs(3));
        let t = Instant::now();
        match c.health_check() {
            Ok(v) => {
                let ms = t.elapsed().as_millis() as u64;
                let search = match c.call(
                    "search.semantic",
                    serde_json::json!({"query": "sonde", "limit": 1}),
                ) {
                    Ok(_) => "recherche routée".to_owned(),
                    Err(PortError::NotWired(_)) => "recherche NON routée côté serveur".to_owned(),
                    Err(e) => format!("recherche : {e}"),
                };
                BrickReport {
                    brick,
                    state: BrickState::Up(format!("maieutik {v} — {search}")),
                    latency_ms: Some(ms),
                }
            }
            Err(e) => BrickReport {
                brick,
                state: BrickState::Unavailable(e.to_string()),
                latency_ms: None,
            },
        }
    }

    fn probe_laya(&mut self) -> Vec<BrickReport> {
        let heads = [Brick::LayaNoul, Brick::LayaChoice, Brick::LayaScore];
        let all = |state: BrickState, ms: Option<u64>| -> Vec<BrickReport> {
            heads
                .iter()
                .map(|b| BrickReport {
                    brick: *b,
                    state: state.clone(),
                    latency_ms: ms,
                })
                .collect()
        };
        if self.cfg.demo {
            return all(BrickState::Mock("verdicts simulés".into()), None);
        }
        let Some(laya) = self.laya.as_mut() else {
            let why = self
                .laya_err
                .as_ref()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "inconnu".into());
            return all(BrickState::Unavailable(why), None);
        };
        let t = Instant::now();
        let health = match laya.health() {
            Ok(h) => h,
            Err(e) => return all(BrickState::Unavailable(e.to_string()), None),
        };
        let ms = Some(t.elapsed().as_millis() as u64);
        let noise = if laya.noise_lines() > 0 {
            format!(
                " ⚠ {} ligne(s) non-JSON sur stdout du serveur",
                laya.noise_lines()
            )
        } else {
            String::new()
        };
        if laya.mock {
            let s = BrickState::Mock(format!(
                "{} en mode simulé — aucune protection réelle{noise}",
                laya.server_label()
            ));
            return all(s, ms);
        }
        let loaded = [
            health.noul_loaded,
            health.choice_loaded,
            health.score_loaded,
        ];
        heads
            .iter()
            .zip(loaded)
            .map(|(b, ok)| BrickReport {
                brick: *b,
                state: if ok {
                    BrickState::Up(format!("{} — tête chargée{noise}", laya.server_label()))
                } else {
                    BrickState::Unavailable(format!(
                        "{} — tête non chargée{noise}",
                        laya.server_label()
                    ))
                },
                latency_ms: ms,
            })
            .collect()
    }

    fn probe_odysseus(&self) -> BrickReport {
        let brick = Brick::Odysseus;
        let Some(root) = self.cfg.odysseus_root.clone() else {
            return BrickReport {
                brick,
                state: BrickState::Unavailable("racine Odysseus non configurée".into()),
                latency_ms: None,
            };
        };
        let state = match (OdysseusProbe { root }).inspect() {
            Ok(d) => BrickState::NotWired(format!(
                "{d} ; aucun contrat UI→Odysseus (Odysseus appelle Maieutik, pas l'inverse)"
            )),
            Err(e) => BrickState::Unavailable(e.to_string()),
        };
        BrickReport {
            brick,
            state,
            latency_ms: None,
        }
    }

    fn run_pipeline(&mut self, prompt: &str) -> Trace {
        self.ensure_laya();
        let cfg = self.cfg.pipeline.clone();
        let mut mock_s1 = MockSystem1;
        let mut mock_search = MockSearch;
        let mut daemon = self
            .cfg
            .daemon_socket
            .clone()
            .map(|p| DaemonClient::new(p, Duration::from_secs(10)).with_mode(cfg.search_mode));
        let mut odysseus = self
            .cfg
            .odysseus_root
            .clone()
            .map(|root| OdysseusProbe { root });

        let demo = self.cfg.demo;
        let system1: Option<&mut dyn System1Port> = if demo {
            Some(&mut mock_s1)
        } else {
            self.laya.as_mut().map(|l| l as &mut dyn System1Port)
        };
        let search: Option<&mut dyn SearchPort> = if demo {
            Some(&mut mock_search)
        } else {
            daemon.as_mut().map(|d| d as &mut dyn SearchPort)
        };
        let agent: Option<&mut dyn AgentPort> = odysseus.as_mut().map(|o| o as &mut dyn AgentPort);
        Pipeline {
            cfg: &cfg,
            system1,
            search,
            agent,
            generator: None,
        }
        .run(prompt)
    }
}
