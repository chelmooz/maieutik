//! Moteurs simulés : mode démo et tests. Tout est marqué `simulated` dans la trace.

use crate::ports::*;

pub struct MockSystem1;

impl System1Port for MockSystem1 {
    fn is_mock(&self) -> bool {
        true
    }

    fn noul(&mut self, text: &str) -> Result<NoulVerdict, PortError> {
        let bad = text.to_lowercase().contains("ignore previous instructions");
        Ok(NoulVerdict {
            is_safe: !bad,
            injection_score: if bad { 0.97 } else { 0.01 },
            null_score: 0.0,
            is_ood: false,
            ood_score: 0.05,
            latency_ms: 0,
        })
    }

    fn choice(&mut self, context: &str, options: &[String]) -> Result<ChoiceVerdict, PortError> {
        let c = context.to_lowercase();
        let want = if c.contains("script") || c.contains("agent") || c.contains("code") {
            crate::pipeline::ROUTE_AGENT
        } else if c.contains("bonjour") || c.contains("merci") {
            crate::pipeline::ROUTE_DIRECT
        } else {
            crate::pipeline::ROUTE_RAG
        };
        let idx = options.iter().position(|o| o == want).unwrap_or(0);
        let mut probs = vec![0.1_f32; options.len()];
        probs[idx] = 1.0 - 0.1 * (options.len() as f32 - 1.0);
        Ok(ChoiceVerdict {
            selected_index: idx,
            selected_option: options[idx].clone(),
            probabilities: probs,
            entropy: 0.5,
            confidence: 0.8,
            latency_ms: 0,
        })
    }

    fn score(&mut self, query: &str, chunks: &[String]) -> Result<ScoreVerdict, PortError> {
        let q: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_owned).collect();
        let scores: Vec<ChunkScore> = chunks
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let lc = c.to_lowercase();
                let hit = q.iter().filter(|w| lc.contains(w.as_str())).count() as f32;
                let rel = (hit / q.len().max(1) as f32).min(1.0);
                ChunkScore { chunk_index: i, text_preview: c.chars().take(40).collect(), relevance: rel, is_relevant: rel > 0.5 }
            })
            .collect();
        let relevant = scores.iter().filter(|s| s.is_relevant).count();
        Ok(ScoreVerdict { chunks_evaluated: scores.len(), chunks_relevant: relevant, scores, latency_ms: 0 })
    }
}

/// Corpus jouet : `sources/`, `wiki/` et `eval/` (ce dernier doit être filtré).
/// Les valeurs sont des similarités `[0,1]` (1 = meilleur) : `eval/` est
/// délibérément le meilleur score, pour prouver que le filtre le retire.
pub struct MockSearch;

impl SearchPort for MockSearch {
    fn search(&mut self, _query: &str, k: usize) -> Result<Vec<Hit>, PortError> {
        let corpus = [
            ("/notes/sources/contrat_client.pdf", "Le contrat client décrit l'architecture réseau et les SLA.", 0.69, Some("Contrat > Réseau")),
            ("/notes/wiki/architecture.md", "Synthèse : Laya route, Maieutik retrouve, Odysseus orchestre.", 0.58, Some("Architecture")),
            ("/notes/eval/questions.md", "Q: quel est le SLA ? R: 99.9 % (réponse attendue — ne doit jamais sortir).", 0.80, None),
            ("/notes/sources/logs_serveur.md", "Logs du serveur : timeouts répétés sur le pont MCP.", 0.45, Some("Logs")),
            ("/notes/sources/recette_crepes.md", "Farine, œufs, lait.", 0.07, None),
        ];
        Ok(corpus
            .iter()
            .take(k)
            .map(|(p, t, d, h)| Hit::new((*p).into(), (*t).into(), *d, h.map(str::to_owned)))
            .collect())
    }
}
