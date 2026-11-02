#![allow(unused)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use laya_primitives::{BatchRequest, LayaConfig, LayaSystem};
use std::sync::Arc;
use tokio::runtime::Runtime;

fn bench_encoder(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = LayaConfig {
        model_path: "models/modernbert-base.onnx".into(),
        tokenizer_path: "models/tokenizer.json".into(),
        ..Default::default()
    };

    // Skip if model not present
    if !std::path::Path::new(&config.model_path).exists() {
        eprintln!("Modèle ONNX non trouvé, benchmarks ignorés");
        return;
    }

    let system = rt.block_on(LayaSystem::load(config)).unwrap();
    let encoder = system.encoder.clone();

    let mut group = c.benchmark_group("encoder");
    for batch_size in [1, 4, 8, 16, 32] {
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, &bs| {
                let texts: Vec<String> = (0..bs)
                    .map(|i| format!("Texte de test numéro {} pour benchmark encoder", i))
                    .collect();
                let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

                b.to_async(&rt)
                    .iter(|| async { black_box(encoder.encode(&text_refs).await.unwrap()) });
            },
        );
    }
    group.finish();
}

fn bench_noul(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = LayaConfig {
        model_path: "models/modernbert-base.onnx".into(),
        tokenizer_path: "models/tokenizer.json".into(),
        ..Default::default()
    };

    if !std::path::Path::new(&config.model_path).exists() {
        return;
    }

    let system = rt.block_on(LayaSystem::load(config)).unwrap();

    let mut group = c.benchmark_group("noul");
    for batch_size in [1, 4, 8, 16, 32] {
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, &bs| {
                let texts: Vec<String> = (0..bs)
                    .map(|i| format!("Requête utilisateur {}", i))
                    .collect();

                b.to_async(&rt).iter(|| async {
                    let mut outputs = Vec::with_capacity(bs);
                    for text in &texts {
                        outputs.push(system.noul(text).await.unwrap());
                    }
                    black_box(outputs)
                });
            },
        );
    }
    group.finish();
}

fn bench_choice(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = LayaConfig {
        model_path: "models/modernbert-base.onnx".into(),
        tokenizer_path: "models/tokenizer.json".into(),
        ..Default::default()
    };

    if !std::path::Path::new(&config.model_path).exists() {
        return;
    }

    let system = rt.block_on(LayaSystem::load(config)).unwrap();
    let options = vec![
        "rag_contracts".into(),
        "rag_infra".into(),
        "agent_code".into(),
        "direct_answer".into(),
    ];

    let mut group = c.benchmark_group("choice");
    for batch_size in [1, 4, 8, 16] {
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, &bs| {
                let contexts: Vec<String> = (0..bs)
                    .map(|i| format!("Contexte de la requête {}", i))
                    .collect();

                b.to_async(&rt).iter(|| async {
                    let mut outputs = Vec::with_capacity(bs);
                    for ctx in &contexts {
                        outputs.push(system.choice(ctx, &options).await.unwrap());
                    }
                    black_box(outputs)
                });
            },
        );
    }
    group.finish();
}

fn bench_score(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = LayaConfig {
        model_path: "models/modernbert-base.onnx".into(),
        tokenizer_path: "models/tokenizer.json".into(),
        ..Default::default()
    };

    if !std::path::Path::new(&config.model_path).exists() {
        return;
    }

    let system = rt.block_on(LayaSystem::load(config)).unwrap();
    let chunks: Vec<String> = (0..10)
        .map(|i| format!("Chunk de document numéro {} avec du contenu technique", i))
        .collect();

    let mut group = c.benchmark_group("score");
    for n_chunks in [5, 10, 20, 32] {
        group.bench_with_input(
            BenchmarkId::from_parameter(n_chunks),
            &n_chunks,
            |b, &nc| {
                let query = "Quelle est l'architecture du système ?";
                let test_chunks = chunks[..nc].to_vec();

                b.to_async(&rt)
                    .iter(|| async { black_box(system.score(query, &test_chunks).await.unwrap()) });
            },
        );
    }
    group.finish();
}

fn bench_batch_noul(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = LayaConfig {
        model_path: "models/modernbert-base.onnx".into(),
        tokenizer_path: "models/tokenizer.json".into(),
        ..Default::default()
    };

    if !std::path::Path::new(&config.model_path).exists() {
        return;
    }

    let system = Arc::new(rt.block_on(LayaSystem::load(config)).unwrap());
    let batch = system.batch.clone();
    batch.spawn_workers(system.clone());

    let mut group = c.benchmark_group("batch_noul");
    for batch_size in [8, 16, 32] {
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, &bs| {
                let texts: Vec<String> = (0..bs).map(|i| format!("Batch test {}", i)).collect();

                b.to_async(&rt)
                    .iter(|| async { black_box(batch.submit_noul(texts.clone()).await.unwrap()) });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_encoder,
    bench_noul,
    bench_choice,
    bench_score,
    bench_batch_noul
);
criterion_main!(benches);
