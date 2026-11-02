//! Interface egui. Toute la logique d'état vit dans `ui()` (testable sans
//! fenêtre) ; `eframe::App::update` ne fait que l'appeler.

use crate::backend::{Backend, BackendConfig, Command, Event};
use crate::collections::Collection;
use crate::egress::EgressPolicy;
use crate::pipeline::{GuardPolicy, Outcome, PipelineConfig, StepStatus, Trace};
use crate::ports::SearchMode;
use crate::status::{BrickReport, BrickState, Host};
use eframe::egui::{self, Color32, RichText};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Query,
    Network,
}

const GREEN: Color32 = Color32::from_rgb(90, 200, 130);
const AMBER: Color32 = Color32::from_rgb(235, 175, 55);
const RED: Color32 = Color32::from_rgb(230, 90, 90);
const BLUE: Color32 = Color32::from_rgb(130, 150, 215);

fn state_color(s: &BrickState) -> Color32 {
    match s {
        BrickState::Up(_) => GREEN,
        BrickState::Mock(_) => AMBER,
        BrickState::Unavailable(_) => RED,
        BrickState::NotWired(_) => BLUE,
    }
}

fn status_color(s: &StepStatus) -> Color32 {
    match s {
        StepStatus::Ok => GREEN,
        StepStatus::Skipped(_) => BLUE,
        StepStatus::Unavailable(_) => AMBER,
        StepStatus::Blocked(_) | StepStatus::Failed(_) => RED,
    }
}

fn status_text(s: &StepStatus) -> String {
    match s {
        StepStatus::Ok => "OK".into(),
        StepStatus::Skipped(w) => format!("IGNORÉ — {w}"),
        StepStatus::Unavailable(w) => format!("INDISPONIBLE — {w}"),
        StepStatus::Blocked(w) => format!("BLOQUÉ — {w}"),
        StepStatus::Failed(w) => format!("ÉCHEC — {w}"),
    }
}

pub struct App {
    backend: Backend,
    cfg: BackendConfig,
    tab: Tab,
    health: Vec<BrickReport>,
    trace: Option<Trace>,
    prompt: String,
    pipeline: PipelineConfig,
    use_sources: bool,
    use_wiki: bool,
    use_other: bool,
    url_probe: String,
    busy: bool,
}

impl App {
    pub fn new(ctx: egui::Context, cfg: BackendConfig) -> Result<Self, crate::egress::EgressError> {
        ctx.set_visuals(egui::Visuals::dark());
        let wake_ctx = ctx.clone();
        let backend = Backend::start(cfg.clone(), move || wake_ctx.request_repaint())?;
        backend.send(Command::Probe);
        Ok(Self {
            backend,
            pipeline: cfg.pipeline.clone(),
            cfg,
            tab: Tab::Dashboard,
            health: Vec::new(),
            trace: None,
            prompt: String::new(),
            use_sources: true,
            use_wiki: true,
            use_other: true,
            url_probe: "http://localhost:11434/v1".into(),
            busy: false,
        })
    }

    pub fn set_tab(&mut self, t: Tab) {
        self.tab = t;
    }

    pub fn set_prompt(&mut self, p: &str) {
        self.prompt = p.to_owned();
    }

    pub fn submit(&mut self) {
        if self.busy || self.prompt.trim().is_empty() {
            return;
        }
        let mut cols = Vec::new();
        for (on, c) in [
            (self.use_sources, Collection::Sources),
            (self.use_wiki, Collection::Wiki),
            (self.use_other, Collection::Other),
        ] {
            if on {
                cols.push(c);
            }
        }
        self.pipeline.collections = cols;
        self.backend.send(Command::SetPipeline(self.pipeline.clone()));
        self.backend.send(Command::Run(self.prompt.clone()));
        self.busy = true;
    }

    pub fn has_health(&self) -> bool {
        !self.health.is_empty()
    }

    pub fn has_trace(&self) -> bool {
        self.trace.is_some()
    }

    fn poll(&mut self) {
        while let Ok(ev) = self.backend.rx.try_recv() {
            match ev {
                Event::Health(h) => self.health = h,
                Event::Trace(t) => {
                    self.trace = Some(*t);
                    self.busy = false;
                }
            }
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        self.poll();

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Maieutik");
                ui.label(RichText::new("local-first · Laya · Odysseus").weak());
                if self.cfg.demo {
                    ui.label(RichText::new("  MODE DÉMO — résultats simulés").color(AMBER).strong());
                }
            });
        });

        egui::SidePanel::left("nav").resizable(false).exact_width(180.0).show(ctx, |ui| {
            ui.add_space(8.0);
            ui.selectable_value(&mut self.tab, Tab::Dashboard, "Tableau de bord");
            ui.selectable_value(&mut self.tab, Tab::Query, "Requête & RAG");
            ui.selectable_value(&mut self.tab, Tab::Network, "Réseau & garde-fous");
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| match self.tab {
                Tab::Dashboard => self.dashboard(ui),
                Tab::Query => self.query(ui),
                Tab::Network => self.network(ui),
            });
        });
    }

    fn dashboard(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("État des briques");
            if ui.button("Sonder").clicked() {
                self.backend.send(Command::Probe);
            }
        });
        ui.label(
            RichText::new("Chaque pastille est une mesure : UP exige une réponse réelle ; MOCK = moteur simulé ; NON CÂBLÉ = contrat absent.")
                .weak(),
        );
        ui.add_space(6.0);
        if self.health.is_empty() {
            ui.label("Sonde en cours…");
            return;
        }
        for host in [Host::Docker1, Host::Docker2, Host::HostApp] {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(host.label()).strong());
                ui.separator();
                for r in self.health.iter().filter(|r| r.brick.host() == host) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("{:<12}", r.state.tag())).color(state_color(&r.state)).strong().monospace());
                        ui.label(RichText::new(r.brick.label()).strong());
                        if let Some(ms) = r.latency_ms {
                            ui.label(RichText::new(format!("{ms} ms")).monospace().weak());
                        }
                    });
                    ui.label(RichText::new(r.state.detail()).weak().small());
                    ui.add_space(4.0);
                }
            });
            ui.add_space(6.0);
        }
    }

    fn query(&mut self, ui: &mut egui::Ui) {
        ui.heading("Requête");
        ui.add(
            egui::TextEdit::multiline(&mut self.prompt)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text("Pose une question à tes notes… (Ctrl+Entrée pour exécuter)"),
        );
        let ctrl_enter = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));

        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_label("Garde-fou")
                .selected_text(match self.pipeline.guard {
                    GuardPolicy::RequireReal => "Noul réel requis (fail-closed)",
                    GuardPolicy::AllowMock => "Noul simulé accepté",
                    GuardPolicy::Skip => "Désactivé",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.pipeline.guard, GuardPolicy::RequireReal, "Noul réel requis (fail-closed)");
                    ui.selectable_value(&mut self.pipeline.guard, GuardPolicy::AllowMock, "Noul simulé accepté");
                    ui.selectable_value(&mut self.pipeline.guard, GuardPolicy::Skip, "Désactivé");
                });
            ui.checkbox(&mut self.pipeline.rerank, "Reranking Laya Score");
            ui.add(egui::Slider::new(&mut self.pipeline.top_k, 1..=10).text("top-k"));
            egui::ComboBox::from_label("Recherche")
                .selected_text(self.pipeline.search_mode.label())
                .show_ui(ui, |ui| {
                    for m in [SearchMode::Hybrid, SearchMode::Semantic, SearchMode::Lexical] {
                        ui.selectable_value(&mut self.pipeline.search_mode, m, m.label());
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Collections :");
            ui.checkbox(&mut self.use_sources, "sources");
            ui.checkbox(&mut self.use_wiki, "wiki");
            ui.checkbox(&mut self.use_other, "autre");
            ui.label(RichText::new("eval/ — toujours exclu").weak());
        });

        let can = !self.busy && !self.prompt.trim().is_empty();
        let clicked = ui.add_enabled(can, egui::Button::new(if self.busy { "En cours…" } else { "Exécuter" })).clicked();
        if clicked || (ctrl_enter && can) {
            self.submit();
        }
        ui.separator();

        let Some(t) = &self.trace else {
            ui.label(RichText::new("Aucune exécution.").weak());
            return;
        };
        outcome_banner(ui, t);
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "Trace — route : {} — {} ms au total",
                t.route.as_deref().unwrap_or("(non atteinte)"),
                t.total_ms
            ))
            .strong(),
        );
        egui::Grid::new("trace").striped(true).num_columns(4).spacing([12.0, 4.0]).show(ui, |ui| {
            for s in &t.steps {
                ui.label(s.name);
                ui.label(RichText::new(format!("{} ms", s.ms)).monospace());
                let mut txt = RichText::new(status_text(&s.status)).color(status_color(&s.status));
                if s.simulated {
                    txt = txt.italics();
                }
                ui.label(txt);
                let mut d = s.detail.clone();
                if s.simulated {
                    d.push_str(" [simulé]");
                }
                ui.label(RichText::new(d).weak());
                ui.end_row();
            }
        });

        if !t.hits.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new(format!("Passages ({})", t.hits.len())).strong());
            for (i, h) in t.hits.iter().enumerate() {
                let prov = if h.collection.is_primary() { "source primaire" } else { "synthèse / autre" };
                let rel = h.relevance.map(|r| format!(" · pertinence {r:.2}")).unwrap_or_default();
                let head = format!("{}. [{}] {}  (score {:.3}{rel})", i + 1, h.collection.label(), h.file_path, h.score);
                egui::CollapsingHeader::new(head).id_salt(("hit", i)).default_open(i == 0).show(ui, |ui| {
                    if let Some(hp) = &h.heading_path {
                        ui.label(RichText::new(hp).weak());
                    }
                    ui.label(&h.text);
                    ui.label(RichText::new(prov).weak().small());
                });
            }
        }
    }

    fn network(&mut self, ui: &mut egui::Ui) {
        ui.heading("Réseau & garde-fous");
        ui.label("L'UI ne parle qu'à la machine locale. Tout le reste est refusé avant connexion.");
        ui.add_space(6.0);
        egui::Grid::new("channels").striped(true).num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
            let sock = self.cfg.daemon_socket.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(aucun)".into());
            let laya = self.cfg.laya.as_ref().map(|l| l.launch.program.display().to_string()).unwrap_or_else(|| "(non configuré)".into());
            let od = self.cfg.odysseus_root.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(non configuré)".into());
            let llm = self.cfg.llm_endpoint.clone().unwrap_or_else(|| "(non configuré)".into());
            for (k, v) in [
                ("Maieutik — socket Unix", sock),
                ("Laya — processus fils (stdio MCP)", laya),
                ("Odysseus — racine (présence seulement)", od),
                ("LLM System 2 — endpoint (loopback)", llm),
            ] {
                ui.label(k);
                ui.label(RichText::new(v).monospace());
                ui.end_row();
            }
        });
        ui.add_space(10.0);
        ui.label(RichText::new("Tester une URL contre la politique").strong());
        ui.add(egui::TextEdit::singleline(&mut self.url_probe).desired_width(f32::INFINITY));
        match EgressPolicy::default().check_url(&self.url_probe) {
            Ok(()) => ui.label(RichText::new("AUTORISÉE — boucle locale").color(GREEN).strong()),
            Err(e) => ui.label(RichText::new(format!("REFUSÉE — {e}")).color(RED).strong()),
        };
        ui.add_space(10.0);
        ui.label(RichText::new("Mutations : aucune. L'UI lit et trace ; elle ne modifie ni l'index ni la configuration de production.").weak());
    }
}

fn outcome_banner(ui: &mut egui::Ui, t: &Trace) {
    let (txt, col) = match &t.outcome {
        Outcome::Halted(w) => (format!("ARRÊTÉ — {w}"), RED),
        Outcome::Retrieved(n) => (format!("{n} passage(s) retrouvé(s) — génération non câblée, aucune réponse rédigée"), AMBER),
        Outcome::AgentUnavailable(w) => (format!("Route agent choisie, Odysseus indisponible — {w}"), AMBER),
        Outcome::DirectUnavailable(w) => (format!("Réponse directe demandée — {w}"), AMBER),
        Outcome::Failed(w) => (format!("ÉCHEC — {w}"), RED),
    };
    ui.label(RichText::new(txt).color(col).strong());
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}
