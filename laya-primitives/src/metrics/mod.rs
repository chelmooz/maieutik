//! Métriques Prometheus pour les primitives System 1
//!
//! Expose : latence (histogramme), throughput (counter), erreurs, distribution des scores.

use super::LayaError;
use prometheus::{Histogram, HistogramOpts, IntCounter, IntGauge, Opts, Registry};
use std::sync::Arc;
use std::time::Instant;

/// Handle pour mesurer une latence (RAII).
pub struct Timer {
    histogram: Histogram,
    start: Instant,
}

impl Timer {
    pub fn new(histogram: Histogram) -> Self {
        Self {
            histogram,
            start: Instant::now(),
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.histogram.observe(self.start.elapsed().as_secs_f64());
    }
}

/// Métriques agrégées pour toutes les primitives.
pub struct PrimitiveMetrics {
    pub registry: Registry,

    // Noul
    pub noul_total: IntCounter,
    pub noul_safe: IntCounter,
    pub noul_unsafe: IntCounter,
    pub noul_latency: Histogram,
    pub noul_injection_score: Histogram,
    pub noul_null_score: Histogram,
    pub noul_ood_score: Histogram,

    // Choice
    pub choice_total: IntCounter,
    pub choice_latency: Histogram,
    pub choice_entropy: Histogram,
    pub choice_confidence: Histogram,

    // Score
    pub score_total: IntCounter,
    pub score_latency: Histogram,
    pub score_relevance: Histogram,
    pub score_chunks_relevant: IntCounter,
    pub score_chunks_total: IntCounter,

    // Batch
    pub batch_size: Histogram,
    pub batch_wait_ms: Histogram,

    // Système
    pub encoder_latency: Histogram,
    pub active_requests: IntGauge,
}

impl PrimitiveMetrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        macro_rules! counter {
            ($name:expr, $help:expr) => {
                IntCounter::with_opts(Opts::new($name, $help)).expect("counter")
            };
        }
        macro_rules! gauge {
            ($name:expr, $help:expr) => {
                IntGauge::with_opts(Opts::new($name, $help)).expect("gauge")
            };
        }
        macro_rules! histogram {
            ($name:expr, $help:expr, $buckets:expr) => {
                Histogram::with_opts(HistogramOpts::new($name, $help).buckets($buckets))
                    .expect("histogram")
            };
        }

        let buckets_latency = vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ];
        let buckets_score = vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
        let buckets_entropy = vec![
            0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.5, 2.0, 2.5, 3.0,
        ];
        let buckets_batch = vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0];

        let metrics = Self {
            registry: registry.clone(),

            noul_total: counter!("laya_noul_total", "Total Noul predictions"),
            noul_safe: counter!("laya_noul_safe_total", "Noul predictions marked safe"),
            noul_unsafe: counter!("laya_noul_unsafe_total", "Noul predictions marked unsafe"),
            noul_latency: histogram!(
                "laya_noul_latency_seconds",
                "Noul inference latency",
                buckets_latency.clone()
            ),
            noul_injection_score: histogram!(
                "laya_noul_injection_score",
                "Noul injection score distribution",
                buckets_score.clone()
            ),
            noul_null_score: histogram!(
                "laya_noul_null_score",
                "Noul null score distribution",
                buckets_score.clone()
            ),
            noul_ood_score: histogram!(
                "laya_noul_ood_score",
                "Noul OOD score distribution",
                buckets_score.clone()
            ),

            choice_total: counter!("laya_choice_total", "Total Choice predictions"),
            choice_latency: histogram!(
                "laya_choice_latency_seconds",
                "Choice inference latency",
                buckets_latency.clone()
            ),
            choice_entropy: histogram!(
                "laya_choice_entropy",
                "Choice output entropy",
                buckets_entropy.clone()
            ),
            choice_confidence: histogram!(
                "laya_choice_confidence",
                "Choice confidence (1 - entropy/max)",
                buckets_score.clone()
            ),

            score_total: counter!("laya_score_total", "Total Score predictions"),
            score_latency: histogram!(
                "laya_score_latency_seconds",
                "Score inference latency",
                buckets_latency.clone()
            ),
            score_relevance: histogram!(
                "laya_score_relevance",
                "Score relevance distribution",
                buckets_score.clone()
            ),
            score_chunks_relevant: counter!(
                "laya_score_chunks_relevant_total",
                "Chunks marked relevant"
            ),
            score_chunks_total: counter!("laya_score_chunks_total_total", "Total chunks evaluated"),

            batch_size: histogram!(
                "laya_batch_size",
                "Batch size distribution",
                buckets_batch.clone()
            ),
            batch_wait_ms: histogram!(
                "laya_batch_wait_ms",
                "Batch wait time ms",
                vec![1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0]
            ),

            encoder_latency: histogram!(
                "laya_encoder_latency_seconds",
                "Encoder inference latency",
                buckets_latency
            ),
            active_requests: gauge!("laya_active_requests", "Currently processing requests"),
        };

        // Enregistrer toutes les métriques
        metrics.register_all().expect("Failed to register metrics");
        metrics
    }

    fn register_all(&self) -> Result<(), prometheus::Error> {
        self.registry.register(Box::new(self.noul_total.clone()))?;
        self.registry.register(Box::new(self.noul_safe.clone()))?;
        self.registry.register(Box::new(self.noul_unsafe.clone()))?;
        self.registry
            .register(Box::new(self.noul_latency.clone()))?;
        self.registry
            .register(Box::new(self.noul_injection_score.clone()))?;
        self.registry
            .register(Box::new(self.noul_null_score.clone()))?;
        self.registry
            .register(Box::new(self.noul_ood_score.clone()))?;

        self.registry
            .register(Box::new(self.choice_total.clone()))?;
        self.registry
            .register(Box::new(self.choice_latency.clone()))?;
        self.registry
            .register(Box::new(self.choice_entropy.clone()))?;
        self.registry
            .register(Box::new(self.choice_confidence.clone()))?;

        self.registry.register(Box::new(self.score_total.clone()))?;
        self.registry
            .register(Box::new(self.score_latency.clone()))?;
        self.registry
            .register(Box::new(self.score_relevance.clone()))?;
        self.registry
            .register(Box::new(self.score_chunks_relevant.clone()))?;
        self.registry
            .register(Box::new(self.score_chunks_total.clone()))?;

        self.registry.register(Box::new(self.batch_size.clone()))?;
        self.registry
            .register(Box::new(self.batch_wait_ms.clone()))?;

        self.registry
            .register(Box::new(self.encoder_latency.clone()))?;
        self.registry
            .register(Box::new(self.active_requests.clone()))?;

        Ok(())
    }

    /// Démarre un timer pour une latence.
    pub fn start_timer(&self, histogram: &Histogram) -> Timer {
        self.active_requests.inc();
        Timer::new(histogram.clone())
    }

    /// Enregistre une prédiction Noul.
    pub fn record_noul(&self, output: &super::noul::NoulOutput) {
        self.noul_total.inc();
        if output.is_safe {
            self.noul_safe.inc();
        } else {
            self.noul_unsafe.inc();
        }
        self.noul_injection_score
            .observe(output.injection_score as f64);
        self.noul_null_score.observe(output.null_score as f64);
        self.noul_ood_score.observe(output.ood_score as f64);
        self.active_requests.dec();
    }

    /// Enregistre une prédiction Choice.
    pub fn record_choice(&self, output: &super::choice::ChoiceOutput) {
        self.choice_total.inc();
        self.choice_entropy.observe(output.entropy as f64);
        self.choice_confidence.observe(output.confidence as f64);
        self.active_requests.dec();
    }

    /// Enregistre une prédiction Score.
    pub fn record_score(&self, output: &super::score::ScoreOutput) {
        self.score_total.inc();
        self.score_chunks_total
            .inc_by(output.chunks_evaluated as u64);
        self.score_chunks_relevant
            .inc_by(output.chunks_relevant as u64);
        for cs in &output.scores {
            self.score_relevance.observe(cs.relevance as f64);
        }
        self.active_requests.dec();
    }

    /// Enregistre métriques de batch.
    pub fn record_batch(&self, size: usize, wait_ms: u64) {
        self.batch_size.observe(size as f64);
        self.batch_wait_ms.observe(wait_ms as f64);
    }

    /// Snapshot pour exposition /health ou /metrics.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            noul_total: self.noul_total.get() as i64,
            noul_safe: self.noul_safe.get() as i64,
            noul_unsafe: self.noul_unsafe.get() as i64,
            choice_total: self.choice_total.get() as i64,
            score_total: self.score_total.get() as i64,
            score_chunks_relevant: self.score_chunks_relevant.get() as i64,
            score_chunks_total: self.score_chunks_total.get() as i64,
            active_requests: self.active_requests.get() as i64,
        }
    }
}

/// Snapshot lisible des métriques (pour health check JSON).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct MetricsSnapshot {
    pub noul_total: i64,
    pub noul_safe: i64,
    pub noul_unsafe: i64,
    pub choice_total: i64,
    pub score_total: i64,
    pub score_chunks_relevant: i64,
    pub score_chunks_total: i64,
    pub active_requests: i64,
}

impl Default for PrimitiveMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_snapshot_serializes() {
        let m = PrimitiveMetrics::new();
        let snap = m.snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("noul_total"));
    }
}
