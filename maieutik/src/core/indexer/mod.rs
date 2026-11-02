//! Vector index — ONNX embeddings (BGE-M3) + sled + HNSW (fast-hnsw).

use crate::config::{Chunk, FileContent};
use crate::core::retrieval::{self, Bm25Doc, Bm25Index, SearchHit, SearchMode};
use anyhow::{Context, Result};
use fast_hnsw::distance::Euclidean;
use fast_hnsw::{Builder, Hnsw};
use ort::session::Session;
use ort::value::Tensor;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokenizers::Tokenizer;
use tokio::sync::RwLock;
use tracing::{info, warn};

const VECTOR_TABLE: &str = "vectors";
const META_TABLE: &str = "meta";
/// Empreinte par fichier ({chemin: FileFingerprint}), séparée des vecteurs :
/// ajouter un arbre est additif, alors que changer `VectorRecord` casserait
/// la lecture bincode des index existants.
const FILE_META_TABLE: &str = "file_meta";
const SCHEMA_VERSION: i32 = 2;
/// BGE-M3 (BAAI/bge-m3): XLM-RoBERTa-large backbone, 1024-dim dense vectors.
/// The official ONNX export outputs `sentence_embedding` directly.
const EMBEDDING_DIM: usize = 1024;
const MODEL_ID: &str = "BAAI/bge-m3";
/// Hard cap matching BGE-M3's context window; the tokenizer.json has no
/// truncation configured, so it is applied in code before encoding.
const MAX_SEQ_LEN: usize = 8192;
/// Intra-op threads for the embedder session, overridable with
/// `MAIEUTIK_EMBEDDER_THREADS`. Defaults to the machine's parallelism.
fn embedder_threads() -> usize {
    std::env::var("MAIEUTIK_EMBEDDER_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        })
}

#[derive(Clone)]
pub struct Embedder {
    /// ONNX Runtime sessions require `&mut self` for inference, so the session
    /// is held behind a mutex. `Session` is `Send + Sync` in ort rc.15.
    session: Arc<Mutex<Session>>,
    /// Real tokenizer shipped next to the model (`tokenizer.json`, Unigram for
    /// BGE-M3's XLM-RoBERTa).
    tokenizer: Tokenizer,
    /// Input names declared by the ONNX graph, in graph order.
    input_names: Vec<String>,
}

impl std::fmt::Debug for Embedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Embedder")
            .field("inputs", &self.input_names)
            .field("vocab_size", &self.tokenizer.get_vocab_size(true))
            .finish()
    }
}

impl Embedder {
    /// Load the ONNX model and its matching tokenizer.
    ///
    /// Both files must belong together: the tokenizer defines the vocabulary the
    /// model was trained with, so a mismatch silently degrades every embedding.
    pub fn new(model_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        // Shared process-wide ORT environment (see core::ort_env).
        let session = crate::core::ort_env::build_session(model_path, embedder_threads())?;

        let input_names: Vec<String> = session
            .inputs()
            .iter()
            .map(|outlet| outlet.name().to_string())
            .collect();

        let mut tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow::anyhow!("load tokenizer {}: {e}", tokenizer_path.display()))?;
        // BGE-M3's tokenizer.json ships without truncation; enforce the model's
        // context window (8192) so an oversized chunk fails by truncation, not
        // by an ORT shape error.
        tokenizer
            .with_truncation(Some(tokenizers::TruncationParams {
                strategy: tokenizers::TruncationStrategy::LongestFirst,
                stride: 0,
                max_length: MAX_SEQ_LEN,
                direction: tokenizers::TruncationDirection::Right,
            }))
            .map_err(|e| anyhow::anyhow!("configure truncation: {e}"))?;

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            tokenizer,
            input_names,
        })
    }

    /// Embed a single text → L2-normalized float32 vector.
    ///
    /// BGE-M3 (XLM-RoBERTa tokenizer, no query prefix needed). The official
    /// ONNX export outputs `sentence_embedding` [batch, 1024]; the pooling the
    /// graph performs internally is not independently verified here, so the
    /// code only guarantees L2 normalization on top of whatever the export
    /// produces. If pointed at a raw `last_hidden_state` export instead, the
    /// CLS token is pooled, which is BGE-M3's documented dense recipe. Note:
    /// the sparse (lexical weights) and ColBERT outputs of BGE-M3 are NOT
    /// produced by this export.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("tokenize: {e}"))?;

        let ids: Vec<i64> = encoding.get_ids().iter().map(|v| *v as i64).collect();
        let mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|v| *v as i64)
            .collect();
        let type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|v| *v as i64).collect();

        if ids.is_empty() {
            anyhow::bail!("tokenizer produced no tokens");
        }
        let seq_len = ids.len();
        if mask.len() != seq_len || type_ids.len() != seq_len {
            anyhow::bail!(
                "tokenizer returned inconsistent lengths: ids={seq_len}, mask={}, type_ids={}",
                mask.len(),
                type_ids.len()
            );
        }

        let ids_array = ndarray::Array2::from_shape_vec((1, seq_len), ids)?;
        let mask_array = ndarray::Array2::from_shape_vec((1, seq_len), mask.clone())?;
        let type_array = ndarray::Array2::from_shape_vec((1, seq_len), type_ids)?;

        // Feed exactly the inputs the graph declares (BGE-M3 declares
        // input_ids + attention_mask; XLM-R has no token_type_ids).
        let mut inputs: Vec<(String, ort::session::SessionInputValue)> =
            Vec::with_capacity(self.input_names.len());
        for name in &self.input_names {
            let value = match name.as_str() {
                "input_ids" => Tensor::from_array(ids_array.clone())?,
                "attention_mask" => Tensor::from_array(mask_array.clone())?,
                "token_type_ids" => Tensor::from_array(type_array.clone())?,
                other => anyhow::bail!(
                    "unsupported ONNX input `{other}`; expected input_ids, attention_mask, token_type_ids"
                ),
            };
            inputs.push((name.clone(), value.into()));
        }

        let mut session = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("embedder session mutex poisoned"))?;
        let outputs = session.run(inputs)?;

        // rc.15: `try_extract_tensor` returns `(&Shape, &[f32])`.
        // Validate outputs exist - ONNX model must produce at least one output.
        if outputs.len() == 0 {
            anyhow::bail!("ONNX model produced no outputs - expected at least one for embedding");
        }
        let (shape, data) = outputs[0].try_extract_tensor::<f32>()?;
        let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
        let vec: Vec<f32> = match dims.as_slice() {
            // `sentence_embedding` — the export already pooled for us.
            [1, hidden] | [hidden] if *hidden == EMBEDDING_DIM => data.to_vec(),
            // Raw hidden states: BGE-M3's dense recipe is the CLS token.
            [1, seq, hidden] if *seq > 0 && *hidden == EMBEDDING_DIM => data[..*hidden].to_vec(),
            other => anyhow::bail!(
                "unexpected embedding output shape {other:?} (expected [1, {EMBEDDING_DIM}])"
            ),
        };

        // Validate vector values: no NaN or infinity
        for x in &vec {
            if !x.is_finite() {
                anyhow::bail!("ONNX model produced non-finite embedding value: {}", x);
            }
        }

        // Normalize (cosine similarity via Euclidean distance on unit vectors).
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            Ok(vec.into_iter().map(|x| x / norm).collect())
        } else {
            anyhow::bail!("ONNX model produced zero-norm embedding (empty or invalid input)")
        }
    }

    /// Embed multiple texts in batch
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}

/// Port d'embedding : le vrai modèle ONNX ou un double de test.
pub trait TextEmbedder: Send + Sync + std::fmt::Debug {
    /// Vecteur normalisé L2 pour un texte.
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}

impl TextEmbedder for Embedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Embedder::embed(self, text)
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Embedder::embed_batch(self, texts)
    }
}

/// Double de test déterministe : sac de mots haché en EMBEDDING_DIM dimensions,
/// normalisé L2. Ce n'est PAS un modèle sémantique : deux textes ne sont
/// proches que s'ils partagent des mots.
#[derive(Debug, Default, Clone, Copy)]
pub struct HashEmbedder;

impl HashEmbedder {
    pub fn new() -> Self {
        Self
    }
}

impl TextEmbedder for HashEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut v = vec![0.0f32; EMBEDDING_DIM];
        for word in text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
        {
            let h = blake3::hash(word.to_lowercase().as_bytes());
            let b = h.as_bytes();
            let idx = (u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize) % EMBEDDING_DIM;
            let sign = {
                #[allow(clippy::incompatible_msrv)]
                if b[4].is_multiple_of(2) {
                    1.0
                } else {
                    -1.0
                }
            };
            v[idx] += sign;
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        } else {
            anyhow::bail!("HashEmbedder produced zero-norm embedding (empty or invalid input)")
        }
        Ok(v)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VectorRecord {
    pub id: String, // file_path#chunk_idx
    pub file_path: String,
    pub chunk_index: usize,
    pub text: String,
    pub embedding: Vec<f32>,
    pub heading_path: Option<String>,
    pub chunk_ordinal: usize,
    pub display_text: Option<String>,
    pub index_text: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexMeta {
    pub id: String, // "meta"
    pub schema_version: i32,
    pub model_id: String,
    pub dimensions: usize,
    pub chunker_version: i32,
    pub model_size_bytes: Option<u64>,
    /// When true, the index requires a full reindex (e.g. after schema/model mismatch).
    /// Must be cleared only after a successful `index_all` / rebuild.
    pub needs_reindex: bool,
}

impl Default for IndexMeta {
    fn default() -> Self {
        Self {
            id: "meta".into(),
            schema_version: SCHEMA_VERSION,
            model_id: MODEL_ID.into(),
            dimensions: EMBEDDING_DIM,
            chunker_version: 1,
            model_size_bytes: None,
            needs_reindex: false,
        }
    }
}

/// Empreinte d'un fichier au moment de son indexation : taille et date de
/// dernière modification, telles que le système de fichiers les expose. Sert
/// à détecter, au démarrage du service, un fichier **modifié pendant que
/// Maieutik était arrêté** — cas qu'aucun événement du watcher ne couvre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileFingerprint {
    pub size: u64,
    /// Nanosecondes depuis l'epoch UNIX.
    pub mtime_ns: u128,
}

impl FileFingerprint {
    /// `None` quand la date de modification n'est pas disponible : mieux vaut
    /// « inconnu » (entrée conservée, signalée) qu'une empreinte fausse.
    pub fn from_metadata(meta: &std::fs::Metadata) -> Option<Self> {
        let modified = meta.modified().ok()?;
        let since_epoch = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(Self {
            size: meta.len(),
            mtime_ns: since_epoch.as_nanos(),
        })
    }

    /// Empreinte du fichier, à relever **avant** de lire son contenu : une
    /// écriture survenue pendant la lecture fait alors diverger la date
    /// enregistrée, et la prochaine réconciliation réindexe au lieu de croire
    /// le contenu à jour.
    pub fn read(path: &Path) -> std::io::Result<Option<Self>> {
        Ok(Self::from_metadata(&std::fs::metadata(path)?))
    }
}

/// Remplace atomiquement tous les enregistrements d'un fichier (un seul batch sled).
///
/// Correspondance exacte sur `file_path` : le préfixe `chemin#` seul est ambigu
/// (`a` et `a#1.md`), donc chaque enregistrement scanné est vérifié.
/// Une liste vide purge le fichier.
pub fn replace_file_records(
    tree: &sled::Tree,
    file_path: &str,
    records: &[VectorRecord],
) -> Result<()> {
    let mut batch = sled::Batch::default();
    let prefix = format!("{file_path}#");
    for item in tree.scan_prefix(prefix.as_bytes()) {
        let (key, val) = item?;
        let old: VectorRecord = bincode::deserialize(&val)?;
        if old.file_path == file_path {
            batch.remove(key);
        }
    }
    for r in records {
        batch.insert(r.id.as_bytes(), bincode::serialize(r)?);
    }
    tree.apply_batch(batch)?;
    Ok(())
}

/// Garde-fou de contrat : un vecteur doit avoir exactement la dimension
/// déclarée par l'index et ne contenir que des valeurs finies. Un NaN ou un
/// Inf corrompt silencieusement l'arbre HNSW (distances non comparables) : il
/// est refusé à la frontière plutôt que découvert à la recherche.
fn validate_embedding_vector(vector: &[f32], expected_dim: usize) -> Result<()> {
    if vector.len() != expected_dim {
        anyhow::bail!(
            "dimension {} au lieu de {expected_dim} attendue",
            vector.len()
        );
    }
    if let Some((position, value)) = vector
        .iter()
        .enumerate()
        .find(|(_, value)| !value.is_finite())
    {
        anyhow::bail!("composante {position} non finie ({value})");
    }
    Ok(())
}

/// Wrapper for fast-hnsw index with ID mapping
pub struct HnswIndex {
    inner: Hnsw<Euclidean>,
    dim: usize,
    id_to_idx: HashMap<String, usize>,
    idx_to_id: Vec<String>,
}

impl HnswIndex {
    pub fn new(dim: usize) -> Self {
        let index = Builder::new()
            .m(16)
            .ef_construction(200)
            .seed(42)
            .build(Euclidean);
        Self {
            inner: index,
            dim,
            id_to_idx: HashMap::new(),
            idx_to_id: Vec::new(),
        }
    }

    pub fn add(&mut self, id: String, vec: &[f32]) -> Result<()> {
        validate_embedding_vector(vec, self.dim)
            .with_context(|| format!("vecteur « {id} » refusé"))?;
        let idx = self.inner.insert(vec.to_vec());
        self.id_to_idx.insert(id.clone(), idx);
        self.idx_to_id.push(id);
        Ok(())
    }

    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<(usize, f32)>> {
        validate_embedding_vector(query, self.dim).context("requête de recherche refusée")?;
        // fast-hnsw `search(query, k, ef)` returns `SearchResult { id, distance }`.
        Ok(self
            .inner
            .search(query, k, k.max(64))
            .into_iter()
            .map(|r| (r.id, r.distance))
            .collect())
    }

    pub fn get_id(&self, idx: usize) -> Option<&String> {
        self.idx_to_id.get(idx)
    }

    pub fn len(&self) -> usize {
        self.idx_to_id.len()
    }

    /// Vrai tant qu'aucun vecteur n'a été ajouté (`len() == 0`).
    pub fn is_empty(&self) -> bool {
        self.idx_to_id.is_empty()
    }
}

impl std::fmt::Debug for HnswIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HnswIndex")
            .field("dim", &self.dim)
            .field("len", &self.idx_to_id.len())
            .finish()
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct VectorIndex {
    db: sled::Db,
    vectors_tree: sled::Tree,
    meta_tree: sled::Tree,
    file_meta_tree: sled::Tree,
    hnsw: Arc<RwLock<Option<HnswIndex>>>,
    /// Index lexical reconstruit depuis sled, comme HNSW.
    bm25: Arc<RwLock<Option<Bm25Index>>>,
    embedder: Arc<dyn TextEmbedder>,
}

impl VectorIndex {
    /// Ouvre (ou crée) l'index dans `data_dir` avec `embedder` pour les
    /// vecteurs. Aucun réglage n'est lu ici : les `Folders` et options de
    /// découpage appartiennent à `core::scan`.
    ///
    /// La base de données est maintenue dans le struct pour garder le verrou vivant.
    pub fn open(data_dir: &Path, embedder: impl TextEmbedder + 'static) -> Result<Self> {
        let db = sled::open(data_dir.join("vector_index"))?;
        let vectors_tree = db.open_tree(VECTOR_TABLE)?;
        let meta_tree = db.open_tree(META_TABLE)?;
        let file_meta_tree = db.open_tree(FILE_META_TABLE)?;

        // Load or create metadata
        let meta: IndexMeta = if let Some(bytes) = meta_tree.get("meta")? {
            bincode::deserialize(&bytes)?
        } else {
            let meta = IndexMeta::default();
            meta_tree.insert("meta", bincode::serialize(&meta)?)?;
            meta
        };

        // Verify schema/model compatibility
        if meta.schema_version != SCHEMA_VERSION
            || meta.model_id != MODEL_ID
            || meta.dimensions != EMBEDDING_DIM
        {
            // RUST-003: Instead of clearing data, move vectors to a backup for recovery
            let _backup_tree_name = format!(
                "vectors_backup_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            );

            // Create backup trees and move all vectors + file_meta
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let vectors_backup_name = format!("vectors_backup_{}", timestamp);
            let file_meta_backup_name = format!("file_meta_backup_{}", timestamp);

            warn!(
                "Index schema/model mismatch — moving vectors and file_meta to backup. \
                 Re-indexing is NOT automatic: it must be triggered separately. \
                 Backup trees: {}, {}",
                vectors_backup_name, file_meta_backup_name
            );

            // Backup vectors
            let vectors_backup = db.open_tree(&vectors_backup_name)?;
            for item in vectors_tree.iter() {
                let (key, val) = item?;
                vectors_backup.insert(key, val)?;
            }

            // Backup file_meta
            let file_meta_backup = db.open_tree(&file_meta_backup_name)?;
            for item in file_meta_tree.iter() {
                let (key, val) = item?;
                file_meta_backup.insert(key, val)?;
            }

            // Clear active trees
            vectors_tree.clear()?;
            file_meta_tree.clear()?;

            // Set needs_reindex flag
            let new_meta = IndexMeta {
                needs_reindex: true,
                ..IndexMeta::default()
            };
            meta_tree.insert("meta", bincode::serialize(&new_meta)?)?;
        }

        Ok(Self {
            db,
            vectors_tree,
            meta_tree,
            file_meta_tree,
            hnsw: Arc::new(RwLock::new(None)),
            bm25: Arc::new(RwLock::new(None)),
            embedder: Arc::new(embedder),
        })
    }

    /// Reconstruit les index dérivés : HNSW (dense) et BM25 (lexical).
    ///
    /// Reconstruction complète utilisée au démarrage ou après changement de schéma.
    /// Pour les mises à jour incrémentales, voir `update_bm25_for_file`.
    pub async fn build_indexes(&self) -> Result<()> {
        let mut hnsw = HnswIndex::new(EMBEDDING_DIM);
        let mut docs = Vec::new();

        for item in self.vectors_tree.iter() {
            let (_, val) = item?;
            let record: VectorRecord = bincode::deserialize(&val)?;
            hnsw.add(record.id.clone(), &record.embedding)?;
            docs.push(Bm25Doc {
                id: record.id.clone(),
                file_path: record.file_path.clone(),
                text: record
                    .index_text
                    .as_deref()
                    .unwrap_or(&record.text)
                    .to_string(),
                heading_path: record.heading_path.clone(),
                chunk_index: record.chunk_index,
            });
        }

        let chunks = docs.len();
        info!("Built HNSW index with {chunks} vectors and BM25 index with {chunks} chunks");
        *self.hnsw.write().await = Some(hnsw);
        *self.bm25.write().await = Some(Bm25Index::from_docs(docs));
        Ok(())
    }

    /// Met à jour l'index BM25 pour un fichier spécifique (incrémental).
    ///
    /// Remplace les chunks du fichier dans l'index BM25 sans reconstruire
    /// l'index complet. Si l'index n'existe pas encore, l'initialise.
    /// L'index HNSW n'est PAS mis à jour ici.
    pub async fn update_bm25_for_file(
        &self,
        file_path: &str,
        chunks: &[crate::config::Chunk],
    ) -> Result<()> {
        let mut bm25_guard = self.bm25.write().await;
        let bm25 = bm25_guard.get_or_insert_with(Bm25Index::default);

        let path_str = file_path.to_string();
        if chunks.is_empty() {
            bm25.remove_file(&path_str);
        } else {
            let bm25_docs: Vec<Bm25Doc> = chunks
                .iter()
                .enumerate()
                .map(|(i, chunk)| Bm25Doc {
                    id: format!("{}#{}", path_str, i),
                    file_path: path_str.clone(),
                    text: chunk.text.clone(),
                    heading_path: chunk.heading_path.clone(),
                    chunk_index: i,
                })
                .collect();
            bm25.replace_file(&path_str, bm25_docs);
        }

        if bm25.dead_count() > bm25.len() / 2 && bm25.dead_count() > 100 {
            bm25.compact();
        }

        Ok(())
    }

    /// Supprime un fichier de l'index BM25 (incrémental).
    pub async fn remove_bm25_file(&self, file_path: &str) -> Result<()> {
        let mut bm25_guard = self.bm25.write().await;
        let Some(bm25) = bm25_guard.as_mut() else {
            return Ok(());
        };
        bm25.remove_file(file_path);
        if bm25.dead_count() > bm25.len() / 2 && bm25.dead_count() > 100 {
            bm25.compact();
        }
        Ok(())
    }

    /// Nom historique conservé pour les appelants existants : construit HNSW
    /// **et** BM25.
    pub async fn build_hnsw(&self) -> Result<()> {
        self.build_indexes().await
    }

    /// Upsert a file's chunks: embeddings first, then one atomic replace.
    /// Zero chunks (file emptied) purges the file's previous records.
    /// Met à jour BM25 incrémentalement ; reconstruit HNSW (pas d'incrémental pour fast-hnsw).
    pub async fn upsert_file(&self, file_path: &Path, content: &FileContent) -> Result<()> {
        let fingerprint = FileFingerprint::read(file_path).ok().flatten();
        let path_str = file_path.to_string_lossy().to_string();
        let chunks = content.chunks.clone();
        self.store_file(file_path, content, fingerprint)?;
        // Mise à jour incrémentale BM25
        self.update_bm25_for_file(&path_str, &chunks).await?;
        // HNSW : reconstruction complète (fast-hnsw ne supporte pas l'incrémental)
        self.rebuild_hnsw().await?;
        Ok(())
    }

    /// Reconstruit seulement l'index HNSW (dense), sans toucher à BM25.
    pub async fn rebuild_hnsw(&self) -> Result<()> {
        let mut hnsw = HnswIndex::new(EMBEDDING_DIM);

        for item in self.vectors_tree.iter() {
            let (_, val) = item?;
            let record: VectorRecord = bincode::deserialize(&val)?;
            hnsw.add(record.id.clone(), &record.embedding)?;
        }

        let chunks = hnsw.len();
        info!("Rebuilt HNSW index with {chunks} vectors (BM25 updated incrementally)");
        *self.hnsw.write().await = Some(hnsw);
        Ok(())
    }

    /// Comme `upsert_file`, sans reconstruire HNSW : l'appelant lance
    /// `build_hnsw` une fois à la fin d'un lot.
    /// Écrit les chunks d'un fichier et l'empreinte de la version indexée.
    ///
    /// Ordre d'écriture assumé : les vecteurs d'abord, l'empreinte ensuite.
    /// Un arrêt entre les deux laisse l'ANCIENNE empreinte, et la prochaine
    /// réconciliation réindexe (sur-consommation, jamais de contenu périmé
    /// cru à jour). L'inverse — empreinte neuve, contenu ancien — est
    /// impossible.
    ///
    /// Zéro chunk (fichier vidé) purge les enregistrements ET l'empreinte :
    /// le fichier n'est plus suivi. `None` (empreinte inconnue) retire toute
    /// empreinte précédente : « inconnu » ne doit pas ressembler à « vieux ».
    pub fn store_file(
        &self,
        file_path: &Path,
        content: &FileContent,
        fingerprint: Option<FileFingerprint>,
    ) -> Result<()> {
        let chunks = &content.chunks;
        let path_str = file_path.to_string_lossy().to_string();

        let mut records = Vec::with_capacity(chunks.len());
        if !chunks.is_empty() {
            let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
            let embeddings = self.embedder.embed_batch(&texts)?;
            for (i, (chunk, emb)) in chunks.iter().zip(embeddings).enumerate() {
                records.push(VectorRecord {
                    id: format!("{}#{}", path_str, i),
                    file_path: path_str.clone(),
                    chunk_index: i,
                    text: chunk.text.clone(),
                    embedding: emb,
                    heading_path: chunk.heading_path.clone(),
                    chunk_ordinal: chunk.index,
                    display_text: None,
                    index_text: None,
                });
            }
        }

        replace_file_records(&self.vectors_tree, &path_str, &records)?;
        let key = path_str.as_bytes();
        match fingerprint.filter(|_| !records.is_empty()) {
            Some(fp) => {
                self.file_meta_tree.insert(key, bincode::serialize(&fp)?)?;
            }
            None => {
                self.file_meta_tree.remove(key)?;
            }
        }
        Ok(())
    }

    /// Remove all chunks for a file
    pub async fn remove_file(&self, file_path: &Path) -> Result<()> {
        let path_str = file_path.to_string_lossy().to_string();
        self.purge_file(&path_str)?;
        // Mise à jour incrémentale BM25
        self.remove_bm25_file(&path_str).await?;
        // HNSW : reconstruction complète
        self.rebuild_hnsw().await?;
        Ok(())
    }

    /// Retire les enregistrements d'un fichier SANS reconstruire HNSW.
    /// Pour une purge par lot (réconciliation de `index --force`) :
    /// l'appelant reconstruit l'index une fois à la fin.
    pub fn purge_file(&self, file_path: &str) -> Result<()> {
        replace_file_records(&self.vectors_tree, file_path, &[])?;
        // Symétrique : purger un fichier sans purger son empreinte laisserait
        // croire à une version connue lors d'une future indexation du même
        // chemin.
        self.file_meta_tree.remove(file_path.as_bytes())?;
        Ok(())
    }

    /// Chemins de fichiers distincts présents dans l'index, tels que
    /// stockés (clés de `replace_file_records`). Sert à la réconciliation
    /// de `index --force` : comparer l'index au corpus présent sur disque.
    pub fn stored_file_paths(&self) -> Result<Vec<String>> {
        Ok(self
            .stored_files()?
            .into_iter()
            .map(|(path, _)| path)
            .collect())
    }

    /// Chemins stockés, avec l'empreinte relevée au moment de leur
    /// indexation (`None` = inconnue). Base de la réconciliation de démarrage.
    pub fn stored_files(&self) -> Result<Vec<(String, Option<FileFingerprint>)>> {
        let mut seen = std::collections::HashMap::new();
        for item in self.vectors_tree.iter() {
            let (_, val) = item?;
            let record: VectorRecord = bincode::deserialize(&val)?;
            let fingerprint = self.fingerprint_of(&record.file_path)?;
            seen.insert(record.file_path, fingerprint);
        }
        Ok(seen.into_iter().collect())
    }

    /// Empreinte enregistrée pour un fichier, si l'index en connaît une.
    /// `None` : jamais indexé avec empreinte, ou purgé.
    pub fn fingerprint_of(&self, file_path: &str) -> Result<Option<FileFingerprint>> {
        Ok(self
            .file_meta_tree
            .get(file_path.as_bytes())?
            .and_then(|bytes| bincode::deserialize(&bytes).ok()))
    }

    /// Enregistrement par identifiant de chunk, ou `None` s'il a disparu
    /// entre la construction de l'index et la lecture.
    fn record_by_id(&self, id: &str) -> Result<Option<VectorRecord>> {
        match self.vectors_tree.get(id.as_bytes())? {
            Some(val) => Ok(Some(bincode::deserialize(&val)?)),
            None => Ok(None),
        }
    }

    /// Candidats denses : `(id de chunk, similarité cosinus)`.
    ///
    /// fast-hnsw rend une distance euclidienne ; sur des vecteurs unitaires,
    /// `cos = 1 - d² / 2`. L'erreur d'embedding est propagée : une panne du
    /// modèle ne doit pas passer pour « aucun résultat ».
    async fn semantic_candidates(&self, query: &str, pool: usize) -> Result<Vec<(String, f32)>> {
        let query_emb = self.embedder.embed(query)?;
        let hnsw = self.hnsw.read().await;
        let hnsw = hnsw
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("HNSW not built"))?;
        Ok(hnsw
            .search(&query_emb, pool)?
            .into_iter()
            .filter_map(|(idx, distance)| {
                hnsw.get_id(idx).map(|id| {
                    let similarity = (1.0f32 - distance * distance / 2.0).clamp(0.0, 1.0);
                    (id.clone(), similarity)
                })
            })
            .collect())
    }

    /// Candidats lexicaux : `(id de chunk, score BM25)`.
    async fn lexical_candidates(
        &self,
        analysis: &retrieval::QueryAnalysis,
        pool: usize,
    ) -> Result<Vec<(String, f64)>> {
        let bm25 = self.bm25.read().await;
        let Some(bm25) = bm25.as_ref() else {
            return Ok(Vec::new());
        };
        Ok(bm25
            .search(&analysis.tokens, pool)
            .into_iter()
            .map(|(idx, score)| (bm25.doc(idx).id.clone(), score))
            .collect())
    }

    async fn semantic_hits(&self, query: &str, k: usize) -> Result<Vec<SearchHit>> {
        let pool = (k * 4).max(20);
        let candidates = self.semantic_candidates(query, pool).await?;
        let mut hits = Vec::new();
        for (id, similarity) in candidates.into_iter().take(k) {
            if let Some(record) = self.record_by_id(&id)? {
                hits.push(SearchHit {
                    file_path: record.file_path,
                    text: record.text,
                    heading_path: record.heading_path,
                    chunk_index: record.chunk_index,
                    score: similarity,
                    source: SearchMode::Semantic,
                });
            }
        }
        Ok(hits)
    }

    async fn lexical_hits(
        &self,
        analysis: &retrieval::QueryAnalysis,
        k: usize,
    ) -> Result<Vec<SearchHit>> {
        let pool = (k * 4).max(20);
        let candidates = self.lexical_candidates(analysis, pool).await?;
        let raw: Vec<f64> = candidates.iter().map(|(_, score)| *score).collect();
        let normalized = retrieval::normalize_max(&raw);
        let mut hits = Vec::new();
        for ((id, _), score) in candidates.into_iter().zip(normalized).take(k) {
            if let Some(record) = self.record_by_id(&id)? {
                hits.push(SearchHit {
                    file_path: record.file_path,
                    text: record.text,
                    heading_path: record.heading_path,
                    chunk_index: record.chunk_index,
                    score: score as f32,
                    source: SearchMode::Lexical,
                });
            }
        }
        Ok(hits)
    }

    async fn hybrid_hits(
        &self,
        query: &str,
        analysis: &retrieval::QueryAnalysis,
        k: usize,
    ) -> Result<Vec<SearchHit>> {
        let pool = (k * 4).max(20);
        // HNSW absent → liste dense vide ; une erreur d'embedding, elle, remonte.
        let dense = if self.hnsw.read().await.is_some() {
            self.semantic_candidates(query, pool).await?
        } else {
            Vec::new()
        };
        let lexical = self.lexical_candidates(analysis, pool).await?;
        if dense.is_empty() && lexical.is_empty() {
            return Ok(Vec::new());
        }

        let dense_ids: Vec<String> = dense.iter().map(|(id, _)| id.clone()).collect();
        let lexical_ids: Vec<String> = lexical.iter().map(|(id, _)| id.clone()).collect();
        let fused = retrieval::rrf_fuse(&[
            (dense_ids, analysis.dense_weight),
            (lexical_ids, analysis.lexical_weight),
        ]);
        let fused_raw: Vec<f64> = fused.iter().map(|(_, score)| *score).collect();
        let fused_scores = retrieval::normalize_max(&fused_raw);

        let dense_map: HashMap<&str, f32> = dense
            .iter()
            .map(|(id, score)| (id.as_str(), *score))
            .collect();
        let lexical_rank: HashMap<&str, usize> = lexical
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (id.as_str(), i + 1))
            .collect();

        let mut candidates = Vec::new();
        for ((id, _), score) in fused.iter().zip(fused_scores) {
            if let Some(record) = self.record_by_id(id)? {
                candidates.push(retrieval::Candidate {
                    id: record.id.clone(),
                    file_path: record.file_path.clone(),
                    text: record.text.clone(),
                    heading_path: record.heading_path.clone(),
                    chunk_index: record.chunk_index,
                    score: score as f32,
                    dense_similarity: dense_map.get(id.as_str()).copied(),
                    lexical_rank: lexical_rank.get(id.as_str()).copied(),
                });
            }
        }

        let selected = retrieval::select_passages(
            candidates,
            retrieval::passage::DEFAULT_MAX_TOKENS,
            retrieval::passage::DEFAULT_PER_FILE_LIMIT,
        );
        Ok(selected
            .into_iter()
            .map(|candidate| SearchHit {
                file_path: candidate.file_path,
                text: candidate.text,
                heading_path: candidate.heading_path,
                chunk_index: candidate.chunk_index,
                score: candidate.score,
                source: SearchMode::Hybrid,
            })
            .collect())
    }

    /// Recherche selon le mode demandé.
    ///
    /// `Semantic` garde le comportement historique de [`VectorIndex::search`]
    /// (mais rend une similarité normalisée et la provenance) ; `Lexical`
    /// interroge BM25 seul ; `Hybrid` fusionne les deux listes par RRF puis
    /// sélectionne les passages.
    ///
    /// Une requête sans terme porteur (uniquement des mots vides) rend une
    /// liste vide dans tous les modes : le voisinage dense d'une requête vide
    /// n'est pas une réponse.
    pub async fn search_mode(
        &self,
        query: &str,
        k: usize,
        mode: SearchMode,
    ) -> Result<Vec<SearchHit>> {
        let analysis = retrieval::analyze(query);
        if analysis.tokens.is_empty() {
            return Ok(Vec::new());
        }
        match mode {
            SearchMode::Semantic => self.semantic_hits(query, k).await,
            SearchMode::Lexical => self.lexical_hits(&analysis, k).await,
            SearchMode::Hybrid => self.hybrid_hits(query, &analysis, k).await,
        }
    }

    /// Semantic search — returns top-k (file_path, chunk_text, score)
    pub async fn search(&self, query: &str, k: usize) -> Result<Vec<(String, String, f32)>> {
        let query_emb = self.embedder.embed(query)?;
        let hnsw = self.hnsw.read().await;
        let hnsw = hnsw
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("HNSW not built"))?;

        let results = hnsw.search(&query_emb, k)?;
        let mut out = Vec::new();
        for (idx, score) in results {
            if let Some(id) = hnsw.get_id(idx) {
                // Fetch record by ID
                if let Some(val) = self.vectors_tree.get(id.as_bytes())? {
                    let record: VectorRecord = bincode::deserialize(&val)?;
                    out.push((record.file_path, record.text, score));
                }
            }
        }
        Ok(out)
    }

    pub fn stats(&self) -> (usize, IndexMeta) {
        let count = self.vectors_tree.len();
        let meta: IndexMeta = self
            .meta_tree
            .get("meta")
            .ok()
            .flatten()
            .and_then(|b| bincode::deserialize(&b).ok())
            .unwrap_or_default();
        (count, meta)
    }

    /// Returns true if the index requires a full reindex (e.g. after schema/model mismatch).
    pub fn needs_reindex(&self) -> Result<bool> {
        read_needs_reindex(&self.meta_tree)
    }

    /// Clears the needs_reindex flag after a successful full reindex.
    /// Should be called only after a successful `index_all` or equivalent rebuild.
    pub fn clear_needs_reindex(&self) -> Result<()> {
        let mut meta: IndexMeta = self
            .meta_tree
            .get("meta")
            .ok()
            .flatten()
            .and_then(|b| bincode::deserialize(&b).ok())
            .unwrap_or_default();
        meta.needs_reindex = false;
        self.meta_tree.insert("meta", bincode::serialize(&meta)?)?;
        self.meta_tree.flush()?;
        Ok(())
    }
}

fn read_needs_reindex(meta_tree: &sled::Tree) -> Result<bool> {
    let Some(bytes) = meta_tree.get("meta").context("reading index metadata")? else {
        return Ok(false);
    };
    let meta: IndexMeta = bincode::deserialize(&bytes).context("deserializing index metadata")?;
    Ok(meta.needs_reindex)
}

/// Découpeur de texte conscient des titres.
///
/// Taille et recouvrement sont comptés en caractères ; les positions
/// `byte_start`/`byte_end` restent des indices d'octets valides dans le texte.
pub struct Chunker {
    max_chunk_chars: usize,
    overlap_chars: usize,
}

impl Chunker {
    pub fn new(max_chunk_chars: usize, overlap_chars: usize) -> Self {
        Self {
            max_chunk_chars,
            overlap_chars,
        }
    }

    pub fn chunk(&self, content: &FileContent) -> Vec<Chunk> {
        let text = &content.text;
        // Frontière d'octet de chaque caractère, plus la fin du texte.
        let mut bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        let total = bounds.len();
        bounds.push(text.len());

        let max = self.max_chunk_chars.max(1);
        // Recouvrement < taille : le début de chunk avance toujours.
        let overlap = self.overlap_chars.min(max - 1);

        let mut chunks = Vec::new();
        let mut start = 0;
        while start < total {
            let end = (start + max).min(total);
            let byte_start = bounds[start];
            let byte_end = bounds[end];
            let heading_path = content
                .headings
                .iter()
                .filter(|h| h.byte_offset < byte_end)
                .max_by_key(|h| h.byte_offset)
                .map(|h| h.text.clone());
            chunks.push(Chunk {
                index: chunks.len(),
                text: text[byte_start..byte_end].to_string(),
                heading_path,
                byte_start,
                byte_end,
            });
            if end == total {
                break;
            }
            start = end - overlap;
        }
        chunks
    }
}

pub fn extract_headings(text: &str) -> Vec<crate::config::Heading> {
    let mut headings = Vec::new();
    let mut offset = 0;
    for (line_num, raw) in text.split_inclusive('\n').enumerate() {
        // Retire le saut de ligne (LF ou CRLF) sans toucher à l'offset réel.
        let line = raw.trim_end_matches(['\n', '\r']);
        if let Some(stripped) = line.strip_prefix('#') {
            let level = line.chars().take_while(|c| *c == '#').count();
            if level > 0 && level <= 6 {
                headings.push(crate::config::Heading {
                    level,
                    text: stripped.trim().to_string(),
                    byte_offset: offset,
                    line_number: line_num + 1,
                });
            }
        }
        offset += raw.len();
    }
    headings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_reindex_propagates_metadata_deserialization_errors() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let meta_tree = db.open_tree(META_TABLE).unwrap();
        meta_tree.insert("meta", &[0xff]).unwrap();

        assert!(read_needs_reindex(&meta_tree).is_err());
    }

    #[test]
    fn vector_index_rejects_wrong_dimension_and_nan_inf_embeddings() {
        let mut index = HnswIndex::new(3);

        // Dimension : le message doit nommer l'identifiant, les deux
        // dimensions et l'attendu (diagnostic actionnable).
        let err = index
            .add("chunk-dimension".to_string(), &[1.0, 0.0])
            .unwrap_err();
        let message = format!("{err:#}");
        assert!(
            message.contains("chunk-dimension"),
            "l'identifiant doit apparaître : {message}"
        );
        assert!(
            message.contains("dimension 2 au lieu de 3"),
            "les dimensions doivent apparaître : {message}"
        );

        // Valeurs non finies : refusées avant insertion, jamais dans l'arbre.
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let err = index
                .add("chunk-non-fini".to_string(), &[1.0, value, 0.0])
                .unwrap_err();
            let message = format!("{err:#}");
            assert!(
                message.contains("chunk-non-fini") && message.contains("composante 1"),
                "diagnostic inattendu : {message}"
            );
        }
        assert_eq!(
            index.len(),
            0,
            "un vecteur refusé ne doit pas entrer dans l'index"
        );
        assert!(index.is_empty());

        index
            .add("chunk-valide".to_string(), &[1.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.len(), 1);

        // Le contrat vaut aussi à la recherche : une requête hors dimension
        // ou non finie est une erreur, pas un résultat vide trompeur.
        let err = index.search(&[1.0, 0.0], 1).unwrap_err();
        assert!(
            format!("{err:#}").contains("dimension 2 au lieu de 3"),
            "requête de mauvaise dimension acceptée"
        );
        let err = index.search(&[1.0, f32::NAN, 0.0], 1).unwrap_err();
        assert!(
            format!("{err:#}").contains("composante 1"),
            "requête non finie acceptée"
        );

        let hits = index.search(&[1.0, 0.0, 0.0], 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, 0);
    }
}
