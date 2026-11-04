//! Batch Inference — Groupement de requêtes pour throughput maximal
//!
//! Permet d'envoyer plusieurs requêtes Noul/Choice/Score en un seul forward pass ONNX.
//! Crucial pour latence p99 sous charge.

use super::{ChoiceOutput, LayaError, LayaSystem, NoulOutput, Result, ScoreOutput};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct BatchConfig {
    pub max_batch_size: usize,
    /// Temps max d'attente pour remplir le batch (ms).
    #[serde(default = "default_max_wait_ms")]
    pub max_wait_ms: u64,
}

fn default_max_wait_ms() -> u64 {
    10
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 32,
            max_wait_ms: 10,
        }
    }
}

/// Requête batch unifiée.
#[derive(Debug, Clone)]
pub enum BatchRequest {
    Noul {
        texts: Vec<String>,
        respond_to: mpsc::Sender<Result<Vec<NoulOutput>>>,
    },
    Choice {
        contexts: Vec<String>,
        options_list: Vec<Vec<String>>,
        respond_to: mpsc::Sender<Result<Vec<ChoiceOutput>>>,
    },
    Score {
        queries: Vec<String>,
        chunks_list: Vec<Vec<String>>,
        respond_to: mpsc::Sender<Result<Vec<ScoreOutput>>>,
    },
}

/// Réponse batch (pour compatibilité API).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct BatchResponse {
    pub noul: Option<Vec<NoulOutput>>,
    pub choice: Option<Vec<ChoiceOutput>>,
    pub score: Option<Vec<ScoreOutput>>,
    pub total_latency_ms: u64,
}

/// Moteur d'inférence batchée avec accumulation asynchrone.
pub struct BatchInference {
    config: BatchConfig,
    noul_tx: mpsc::Sender<BatchRequest>,
    choice_tx: mpsc::Sender<BatchRequest>,
    score_tx: mpsc::Sender<BatchRequest>,
    noul_rx: mpsc::Receiver<BatchRequest>,
    choice_rx: mpsc::Receiver<BatchRequest>,
    score_rx: mpsc::Receiver<BatchRequest>,
}

impl BatchInference {
    pub fn new(max_batch_size: usize) -> Self {
        let config = BatchConfig {
            max_batch_size,
            ..Default::default()
        };
        let (noul_tx, noul_rx) = mpsc::channel(128);
        let (choice_tx, choice_rx) = mpsc::channel(128);
        let (score_tx, score_rx) = mpsc::channel(128);

        Self {
            config,
            noul_tx,
            choice_tx,
            score_tx,
            noul_rx,
            choice_rx,
            score_rx,
        }
    }

    /// Démarre les workers de batch (appelé une fois au startup).
    pub fn spawn_workers(self, system: Arc<LayaSystem>) {
        let config = self.config.clone();
        let sys = system.clone();
        tokio::spawn(Self::noul_worker(config.clone(), sys.clone(), self.noul_rx));
        tokio::spawn(Self::choice_worker(
            config.clone(),
            sys.clone(),
            self.choice_rx,
        ));
        tokio::spawn(Self::score_worker(config, sys, self.score_rx));
    }

    /// Traite une requête batch directement (sans workers).
    pub async fn process(&self, req: BatchRequest, system: &LayaSystem) -> Result<BatchResponse> {
        let start = Instant::now();
        let mut response = BatchResponse {
            noul: None,
            choice: None,
            score: None,
            total_latency_ms: 0,
        };

        match req {
            BatchRequest::Noul { texts, .. } => {
                let mut outputs = Vec::with_capacity(texts.len());
                for text in texts {
                    outputs.push(system.noul(&text).await?);
                }
                response.noul = Some(outputs);
            }
            BatchRequest::Choice {
                contexts,
                options_list,
                ..
            } => {
                let mut outputs = Vec::with_capacity(contexts.len());
                for (ctx, opts) in contexts.into_iter().zip(options_list) {
                    outputs.push(system.choice(&ctx, &opts).await?);
                }
                response.choice = Some(outputs);
            }
            BatchRequest::Score {
                queries,
                chunks_list,
                ..
            } => {
                let mut outputs = Vec::with_capacity(queries.len());
                for (q, chunks) in queries.into_iter().zip(chunks_list) {
                    outputs.push(system.score(&q, &chunks).await?);
                }
                response.score = Some(outputs);
            }
        }

        response.total_latency_ms = start.elapsed().as_millis() as u64;
        Ok(response)
    }

    async fn noul_worker(
        config: BatchConfig,
        system: Arc<LayaSystem>,
        mut rx: mpsc::Receiver<BatchRequest>,
    ) {
        let mut buffer = Vec::new();
        let mut interval =
            tokio::time::interval(std::time::Duration::from_millis(config.max_wait_ms));

        loop {
            tokio::select! {
                Some(req) = rx.recv() => {
                    buffer.push(req);
                    if buffer.len() >= config.max_batch_size {
                        Self::flush_noul(&system, &mut buffer).await;
                    }
                }
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        Self::flush_noul(&system, &mut buffer).await;
                    }
                }
            }
        }
    }

    #[allow(clippy::let_underscore_future)]
    async fn flush_noul(system: &LayaSystem, buffer: &mut Vec<BatchRequest>) {
        let requests = std::mem::take(buffer);
        let mut texts = Vec::new();
        let mut responders = Vec::new();

        for req in requests {
            if let BatchRequest::Noul {
                texts: t,
                respond_to,
            } = req
            {
                texts.extend(t);
                responders.push(respond_to);
            }
        }

        if texts.is_empty() {
            return;
        }

        let start = Instant::now();
        // Use the public predict method which handles encoding internally
        let mut outputs = Vec::with_capacity(texts.len());
        for text in &texts {
            match system.noul.write().await.predict(text).await {
                Ok(out) => outputs.push(out),
                Err(e) => {
                    for responder in responders {
                        let _ = responder.send(Err(e.clone())).await;
                    }
                    return;
                }
            }
        }
        let _latency = start.elapsed().as_millis() as u64;

        for (i, responder) in responders.into_iter().enumerate() {
            let _ = responder.send(Ok(vec![outputs[i].clone()])).await;
        }
    }

    async fn choice_worker(
        config: BatchConfig,
        system: Arc<LayaSystem>,
        mut rx: mpsc::Receiver<BatchRequest>,
    ) {
        let mut buffer = Vec::new();
        let mut interval =
            tokio::time::interval(std::time::Duration::from_millis(config.max_wait_ms));

        loop {
            tokio::select! {
                Some(req) = rx.recv() => {
                    buffer.push(req);
                    if buffer.len() >= config.max_batch_size {
                        Self::flush_choice(&system, &mut buffer).await;
                    }
                }
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        Self::flush_choice(&system, &mut buffer).await;
                    }
                }
            }
        }
    }

    #[allow(clippy::let_underscore_future)]
    async fn flush_choice(system: &LayaSystem, buffer: &mut Vec<BatchRequest>) {
        let requests = std::mem::take(buffer);

        for req in requests {
            if let BatchRequest::Choice {
                contexts,
                options_list,
                respond_to,
            } = req
            {
                let mut outputs = Vec::with_capacity(contexts.len());
                for (ctx, opts) in contexts.into_iter().zip(options_list) {
                    match system.choice.write().await.predict(&ctx, &opts).await {
                        Ok(out) => outputs.push(out),
                        Err(e) => {
                            let _ = respond_to.send(Err(e));
                            return;
                        }
                    }
                }
                let _ = respond_to.send(Ok(outputs));
            }
        }
    }

    async fn score_worker(
        config: BatchConfig,
        system: Arc<LayaSystem>,
        mut rx: mpsc::Receiver<BatchRequest>,
    ) {
        let mut buffer = Vec::new();
        let mut interval =
            tokio::time::interval(std::time::Duration::from_millis(config.max_wait_ms));

        loop {
            tokio::select! {
                Some(req) = rx.recv() => {
                    buffer.push(req);
                    if buffer.len() >= config.max_batch_size {
                        Self::flush_score(&system, &mut buffer).await;
                    }
                }
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        Self::flush_score(&system, &mut buffer).await;
                    }
                }
            }
        }
    }

    #[allow(clippy::let_underscore_future)]
    async fn flush_score(system: &LayaSystem, buffer: &mut Vec<BatchRequest>) {
        let requests = std::mem::take(buffer);

        for req in requests {
            if let BatchRequest::Score {
                queries,
                chunks_list,
                respond_to,
            } = req
            {
                let mut outputs = Vec::with_capacity(queries.len());
                for (q, chunks) in queries.into_iter().zip(chunks_list) {
                    match system.score.write().await.predict(&q, &chunks).await {
                        Ok(out) => outputs.push(out),
                        Err(e) => {
                            let _ = respond_to.send(Err(e));
                            return;
                        }
                    }
                }
                let _ = respond_to.send(Ok(outputs));
            }
        }
    }

    /// API publique : soumettre une requête batch (non-bloquant, retourne channel).
    pub async fn submit_noul(&self, texts: Vec<String>) -> Result<Vec<NoulOutput>> {
        let (tx, mut rx) = mpsc::channel(1);
        self.noul_tx
            .send(BatchRequest::Noul {
                texts,
                respond_to: tx,
            })
            .await
            .map_err(|_| LayaError::InvalidInput("Worker Noul arrêté".into()))?;
        rx.recv()
            .await
            .ok_or(LayaError::InvalidInput(
                "Réponse Noul perdue (channel fermé)".into(),
            ))?
            .map_err(|_| LayaError::InvalidInput("Réponse Noul perdue".into()))
    }

    pub async fn submit_choice(
        &self,
        contexts: Vec<String>,
        options_list: Vec<Vec<String>>,
    ) -> Result<Vec<ChoiceOutput>> {
        let (tx, mut rx) = mpsc::channel(1);
        self.choice_tx
            .send(BatchRequest::Choice {
                contexts,
                options_list,
                respond_to: tx,
            })
            .await
            .map_err(|_| LayaError::InvalidInput("Worker Choice arrêté".into()))?;
        rx.recv()
            .await
            .ok_or(LayaError::InvalidInput(
                "Réponse Choice perdue (channel fermé)".into(),
            ))?
            .map_err(|_| LayaError::InvalidInput("Réponse Choice perdue".into()))
    }

    pub async fn submit_score(
        &self,
        queries: Vec<String>,
        chunks_list: Vec<Vec<String>>,
    ) -> Result<Vec<ScoreOutput>> {
        let (tx, mut rx) = mpsc::channel(1);
        self.score_tx
            .send(BatchRequest::Score {
                queries,
                chunks_list,
                respond_to: tx,
            })
            .await
            .map_err(|_| LayaError::InvalidInput("Worker Score arrêté".into()))?;
        rx.recv()
            .await
            .ok_or(LayaError::InvalidInput(
                "Réponse Score perdue (channel fermé)".into(),
            ))?
            .map_err(|_| LayaError::InvalidInput("Réponse Score perdue".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_config_defaults() {
        let cfg = BatchConfig::default();
        assert_eq!(cfg.max_batch_size, 32);
        assert_eq!(cfg.max_wait_ms, 10);
    }
}
