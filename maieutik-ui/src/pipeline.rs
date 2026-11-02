//! Pipeline hybride : Noul → Choice → (RAG | Agent | direct) → génération.
//! Latences mesurées (`Instant`). Étape absente = tracée comme telle ; aucun succès fabriqué.

use crate::collections::Collection;
use crate::ports::*;
use crate::status::Brick;
use std::time::Instant;

pub const ROUTE_RAG: &str = "route_rag_maieutik";
pub const ROUTE_AGENT: &str = "route_agent_odysseus";
pub const ROUTE_DIRECT: &str = "direct_answer";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardPolicy {
    /// Fail-closed : sans Noul réel (non simulé), la requête est stoppée.
    RequireReal,
    /// Accepte un Noul simulé (démo) ; verdict marqué « simulé ».
    AllowMock,
    /// Désactive le garde-fou (explicite, tracé).
    Skip,
}

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub guard: GuardPolicy,
    pub top_k: usize,
    pub overfetch: usize,
    pub rerank: bool,
    pub min_choice_confidence: f32,
    pub collections: Vec<Collection>,
    /// Méthode de recherche envoyée au pont Maieutik.
    pub search_mode: SearchMode,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            guard: GuardPolicy::RequireReal,
            top_k: 5,
            overfetch: 3,
            rerank: true,
            min_choice_confidence: 0.4,
            collections: vec![Collection::Sources, Collection::Wiki, Collection::Other],
            search_mode: SearchMode::Hybrid,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StepStatus {
    Ok,
    Skipped(String),
    Unavailable(String),
    Blocked(String),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct Step {
    pub name: &'static str,
    pub brick: Brick,
    pub status: StepStatus,
    pub ms: u64,
    pub detail: String,
    pub simulated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Halted(String),
    Retrieved(usize),
    AgentUnavailable(String),
    DirectUnavailable(String),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct Trace {
    pub prompt: String,
    pub steps: Vec<Step>,
    pub route: Option<String>,
    pub hits: Vec<Hit>,
    pub outcome: Outcome,
    pub total_ms: u64,
}

pub struct Pipeline<'a> {
    pub cfg: &'a PipelineConfig,
    pub system1: Option<&'a mut dyn System1Port>,
    pub search: Option<&'a mut dyn SearchPort>,
    pub agent: Option<&'a mut dyn AgentPort>,
    pub generator: Option<&'a mut dyn GeneratorPort>,
}

fn route_options() -> Vec<String> {
    [ROUTE_RAG, ROUTE_AGENT, ROUTE_DIRECT].iter().map(|s| s.to_string()).collect()
}

impl Pipeline<'_> {
    pub fn run(&mut self, prompt: &str) -> Trace {
        let t0 = Instant::now();
        let mut tr = Trace {
            prompt: prompt.to_owned(),
            steps: Vec::new(),
            route: None,
            hits: Vec::new(),
            outcome: Outcome::Failed("non exécuté".into()),
            total_ms: 0,
        };
        tr.outcome = self.run_inner(prompt, &mut tr);
        tr.total_ms = t0.elapsed().as_millis() as u64;
        tr
    }

    fn run_inner(&mut self, prompt: &str, tr: &mut Trace) -> Outcome {
        if let Some(halt) = self.step_guard(prompt, tr) {
            return Outcome::Halted(halt);
        }
        let route = self.step_route(prompt, tr);
        tr.route = Some(route.clone());
        match route.as_str() {
            ROUTE_AGENT => self.step_agent(prompt, tr),
            ROUTE_DIRECT => self.step_generate(prompt, tr, ROUTE_DIRECT),
            _ => {
                if let Err(e) = self.step_retrieve(prompt, tr) {
                    return Outcome::Failed(e);
                }
                self.step_generate(prompt, tr, ROUTE_RAG)
            }
        }
    }

    fn push(tr: &mut Trace, name: &'static str, brick: Brick, status: StepStatus, t: Instant, detail: String, simulated: bool) {
        tr.steps.push(Step { name, brick, status, ms: t.elapsed().as_millis() as u64, detail, simulated });
    }

    fn step_guard(&mut self, prompt: &str, tr: &mut Trace) -> Option<String> {
        let t = Instant::now();
        let name = "Noul · garde-fou";
        let b = Brick::LayaNoul;
        if self.cfg.guard == GuardPolicy::Skip {
            Self::push(tr, name, b, StepStatus::Skipped("désactivé par la configuration".into()), t, String::new(), false);
            return None;
        }
        let Some(s1) = self.system1.as_deref_mut() else {
            let why = "Laya non configuré — garde-fou requis (fail-closed)".to_owned();
            Self::push(tr, name, b, StepStatus::Unavailable(why.clone()), t, String::new(), false);
            return Some(why);
        };
        let mock = s1.is_mock();
        if mock && self.cfg.guard == GuardPolicy::RequireReal {
            let why = "Noul simulé : aucune protection réelle — requête stoppée (fail-closed)".to_owned();
            Self::push(tr, name, b, StepStatus::Blocked(why.clone()), t, String::new(), true);
            return Some(why);
        }
        match s1.noul(prompt) {
            Ok(v) if v.is_safe => {
                let d = format!("injection={:.2} ood={:.2}", v.injection_score, v.ood_score);
                Self::push(tr, name, b, StepStatus::Ok, t, d, mock);
                None
            }
            Ok(v) => {
                let why = format!("bloqué : injection={:.2} ood={}", v.injection_score, v.is_ood);
                Self::push(tr, name, b, StepStatus::Blocked(why.clone()), t, String::new(), mock);
                Some(why)
            }
            Err(e) => {
                let why = format!("garde-fou en échec (fail-closed) : {e}");
                Self::push(tr, name, b, StepStatus::Failed(why.clone()), t, String::new(), mock);
                Some(why)
            }
        }
    }

    fn step_route(&mut self, prompt: &str, tr: &mut Trace) -> String {
        let t = Instant::now();
        let name = "Choice · routage";
        let b = Brick::LayaChoice;
        let fallback = |tr: &mut Trace, st: StepStatus, why: String, mock: bool| {
            Self::push(tr, name, b, st, t, why, mock);
            ROUTE_RAG.to_owned()
        };
        let Some(s1) = self.system1.as_deref_mut() else {
            return fallback(tr, StepStatus::Unavailable("Laya absent".into()), "repli explicite sur RAG".into(), false);
        };
        let mock = s1.is_mock();
        match s1.choice(prompt, &route_options()) {
            Ok(v) if v.confidence >= self.cfg.min_choice_confidence => {
                let d = format!("{} (confiance {:.2})", v.selected_option, v.confidence);
                Self::push(tr, name, b, StepStatus::Ok, t, d, mock);
                v.selected_option
            }
            Ok(v) => fallback(
                tr,
                StepStatus::Skipped(format!("confiance {:.2} < {:.2}", v.confidence, self.cfg.min_choice_confidence)),
                format!("proposé : {} — repli sur RAG", v.selected_option),
                mock,
            ),
            Err(e) => fallback(tr, StepStatus::Failed(e.to_string()), "repli explicite sur RAG".into(), mock),
        }
    }

    fn step_retrieve(&mut self, prompt: &str, tr: &mut Trace) -> Result<(), String> {
        let t = Instant::now();
        let b = Brick::Maieutik;
        let Some(search) = self.search.as_deref_mut() else {
            Self::push(tr, "Maieutik · recherche", b, StepStatus::Unavailable("aucun adaptateur de recherche".into()), t, String::new(), false);
            return Ok(());
        };
        let want = self.cfg.top_k * self.cfg.overfetch.max(1);
        let mut hits = match search.search(prompt, want) {
            Ok(h) => h,
            Err(e) if e.is_absent() => {
                Self::push(tr, "Maieutik · recherche", b, StepStatus::Unavailable(e.to_string()), t, String::new(), false);
                return Ok(());
            }
            Err(e) => {
                Self::push(tr, "Maieutik · recherche", b, StepStatus::Failed(e.to_string()), t, String::new(), false);
                return Err(e.to_string());
            }
        };
        let before = hits.len();
        // eval/ n'est jamais servi, même si l'appelant le coche.
        hits.retain(|h| h.collection.allowed_in_production() && self.cfg.collections.contains(&h.collection));
        let dropped = before - hits.len();
        Self::push(tr, "Maieutik · recherche", b, StepStatus::Ok, t, format!("{} · {} passage(s), {dropped} filtré(s) par collection", self.cfg.search_mode.label(), hits.len()), false);

        self.step_rerank(prompt, &mut hits, tr);
        hits.truncate(self.cfg.top_k);
        tr.hits = hits;
        Ok(())
    }

    fn step_rerank(&mut self, prompt: &str, hits: &mut [Hit], tr: &mut Trace) {
        let t = Instant::now();
        let (name, b) = ("Score · reranking", Brick::LayaScore);
        if !self.cfg.rerank {
            Self::push(tr, name, b, StepStatus::Skipped("désactivé".into()), t, String::new(), false);
            return;
        }
        let Some(s1) = self.system1.as_deref_mut() else {
            Self::push(tr, name, b, StepStatus::Unavailable("Laya absent — ordre dense conservé".into()), t, String::new(), false);
            return;
        };
        if hits.is_empty() {
            Self::push(tr, name, b, StepStatus::Skipped("aucun passage".into()), t, String::new(), false);
            return;
        }
        let mock = s1.is_mock();
        let chunks: Vec<String> = hits.iter().map(|h| h.text.clone()).collect();
        match s1.score(prompt, &chunks) {
            Ok(v) if v.scores.len() == hits.len() => {
                for sc in &v.scores {
                    if let Some(h) = hits.get_mut(sc.chunk_index) {
                        h.relevance = Some(sc.relevance);
                    }
                }
                hits.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap_or(std::cmp::Ordering::Equal));
                Self::push(tr, name, b, StepStatus::Ok, t, format!("{} évalué(s)", v.chunks_evaluated), mock);
            }
            Ok(v) => {
                let d = format!("{} scores pour {} passages — ordre dense conservé", v.scores.len(), hits.len());
                Self::push(tr, name, b, StepStatus::Failed(d.clone()), t, d, mock);
            }
            Err(e) => Self::push(tr, name, b, StepStatus::Failed(e.to_string()), t, "ordre dense conservé".into(), mock),
        }
    }

    fn step_agent(&mut self, prompt: &str, tr: &mut Trace) -> Outcome {
        let t = Instant::now();
        let b = Brick::Odysseus;
        let Some(agent) = self.agent.as_deref_mut() else {
            let why = "Odysseus non configuré".to_owned();
            Self::push(tr, "Odysseus · agent", b, StepStatus::Unavailable(why.clone()), t, String::new(), false);
            return Outcome::AgentUnavailable(why);
        };
        match agent.submit(prompt) {
            Ok(_) => {
                Self::push(tr, "Odysseus · agent", b, StepStatus::Ok, t, "tâche déléguée".into(), false);
                Outcome::Retrieved(0)
            }
            Err(e) => {
                Self::push(tr, "Odysseus · agent", b, StepStatus::Unavailable(e.to_string()), t, String::new(), false);
                Outcome::AgentUnavailable(e.to_string())
            }
        }
    }

    fn step_generate(&mut self, prompt: &str, tr: &mut Trace, route: &str) -> Outcome {
        let t = Instant::now();
        let b = Brick::Llm;
        let n = tr.hits.len();
        match self.generator.as_deref_mut() {
            None => {
                let why = "génération System 2 non câblée (jamais exécutée — VERIFICATION.md)".to_owned();
                Self::push(tr, "LLM · génération", b, StepStatus::Unavailable(why.clone()), t, String::new(), false);
                if route == ROUTE_DIRECT { Outcome::DirectUnavailable(why) } else { Outcome::Retrieved(n) }
            }
            Some(g) => match g.answer(prompt, &tr.hits) {
                Ok(_) => {
                    Self::push(tr, "LLM · génération", b, StepStatus::Ok, t, "réponse produite".into(), false);
                    Outcome::Retrieved(n)
                }
                Err(e) => {
                    Self::push(tr, "LLM · génération", b, StepStatus::Failed(e.to_string()), t, String::new(), false);
                    Outcome::Failed(e.to_string())
                }
            },
        }
    }
}
