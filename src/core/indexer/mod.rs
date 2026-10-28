//! Vector index — ONNX embeddings (BGE-M3) + sled + HNSW (fast-hnsw).

use anyhow::Result;
use crate::config::{Chunk, FileContent};
use fast_hnsw::{Builder, Hnsw};
use fast_hnsw::distance::Euclidean;
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

        let mut tokenizer = Tokenizer::from_file(tokenizer_path).map_err(|e| {
            anyhow::anyhow!("load tokenizer {}: {e}", tokenizer_path.display())
        })?;
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
        let (shape, data) = outputs[0].try_extract_tensor::<f32>()?;
        let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
        let vec: Vec<f32> = match dims.as_slice() {
            // `sentence_embedding` — the export already pooled for us.
            [1, hidden] | [hidden] if *hidden == EMBEDDING_DIM => data.to_vec(),
            // Raw hidden states: BGE-M3's dense recipe is the CLS token.
            [1, seq, hidden] if *seq > 0 && *hidden == EMBEDDING_DIM => {
                data[..*hidden].to_vec()
            }
            other => anyhow::bail!(
                "unexpected embedding output shape {other:?} (expected [1, {EMBEDDING_DIM}])"
            ),
        };

        // Normalize (cosine similarity via Euclidean distance on unit vectors).
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            Ok(vec.into_iter().map(|x| x / norm).collect())
        } else {
            Ok(vec)
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
            let sign = if b[4].is_multiple_of(2) { 1.0 } else { -1.0 };
            v[idx] += sign;
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        } else {
            v[0] = 1.0;
        }
        Ok(v)
    }
}


#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VectorRecord {
    pub id: String,           // file_path#chunk_idx
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
    pub id: String,           // "meta"
    pub schema_version: i32,
    pub model_id: String,
    pub dimensions: usize,
    pub chunker_version: i32,
    pub model_size_bytes: Option<u64>,
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


/// Wrapper for fast-hnsw index with ID mapping
pub struct HnswIndex {
    inner: Hnsw<Euclidean>,
    id_to_idx: HashMap<String, usize>,
    idx_to_id: Vec<String>,
}

impl HnswIndex {
    pub fn new(_dim: usize) -> Self {
        let index = Builder::new()
            .m(16)
            .ef_construction(200)
            .seed(42)
            .build(Euclidean);
        Self {
            inner: index,
            id_to_idx: HashMap::new(),
            idx_to_id: Vec::new(),
        }
    }

    pub fn add(&mut self, id: String, vec: &[f32]) -> Result<()> {
        let idx = self.inner.insert(vec.to_vec());
        self.id_to_idx.insert(id.clone(), idx);
        self.idx_to_id.push(id);
        Ok(())
    }

    pub fn search(&self, query: &[f32], k: usize) -> Vec<(usize, f32)> {
        // fast-hnsw `search(query, k, ef)` returns `SearchResult { id, distance }`.
        self.inner
            .search(query, k, k.max(64))
            .into_iter()
            .map(|r| (r.id, r.distance))
            .collect()
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
            .field("len", &self.idx_to_id.len())
            .finish()
    }
}

#[derive(Debug)]
pub struct VectorIndex {
    vectors_tree: sled::Tree,
    meta_tree: sled::Tree,
    file_meta_tree: sled::Tree,
    hnsw: Arc<RwLock<Option<HnswIndex>>>,
    embedder: Arc<dyn TextEmbedder>,
}

impl VectorIndex {
    /// Ouvre (ou crée) l'index dans `data_dir` avec `embedder` pour les
    /// vecteurs. Aucun réglage n'est lu ici : les `Folders` et options de
    /// découpage appartiennent à `core::scan`.
    ///
    /// La poignée `sled::Db` est locale : chaque `Tree` garde le contexte de
    /// la base en vie, donc l'index reste utilisable après sa libération.
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
        if meta.schema_version != SCHEMA_VERSION || meta.model_id != MODEL_ID || meta.dimensions != EMBEDDING_DIM {
            warn!(
                "Index schema/model mismatch — purging stored vectors. \
                 Re-indexing is NOT automatic: it must be triggered separately"
            );
            vectors_tree.clear()?;
            let new_meta = IndexMeta::default();
            meta_tree.insert("meta", bincode::serialize(&new_meta)?)?;
        }

        Ok(Self {
            vectors_tree,
            meta_tree,
            file_meta_tree,
            hnsw: Arc::new(RwLock::new(None)),
            embedder: Arc::new(embedder),
        })
    }

    /// Build/rebuild HNSW index from stored vectors
    pub async fn build_hnsw(&self) -> Result<()> {
        let mut hnsw = HnswIndex::new(EMBEDDING_DIM);
        let mut count = 0;

        for item in self.vectors_tree.iter() {
            let (_, val) = item?;
            let record: VectorRecord = bincode::deserialize(&val)?;
            hnsw.add(record.id.clone(), &record.embedding)?;
            count += 1;
        }

        info!("Built HNSW index with {} vectors", count);
        *self.hnsw.write().await = Some(hnsw);
        Ok(())
    }

    /// Upsert a file's chunks: embeddings first, then one atomic replace.
    /// Zero chunks (file emptied) purges the file's previous records.
    pub async fn upsert_file(&self, file_path: &Path, content: &FileContent) -> Result<()> {
        let fingerprint = FileFingerprint::read(file_path).ok().flatten();
        self.store_file(file_path, content, fingerprint)?;
        self.rebuild_hnsw_if_needed().await?;
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
        self.purge_file(&file_path.to_string_lossy())?;
        self.rebuild_hnsw_if_needed().await?;
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

    async fn rebuild_hnsw_if_needed(&self) -> Result<()> {
        // Simple strategy: rebuild on every change (OK for small indexes)
        // TODO: incremental HNSW updates
        self.build_hnsw().await
    }

    /// Semantic search — returns top-k (file_path, chunk_text, score)
    pub async fn search(&self, query: &str, k: usize) -> Result<Vec<(String, String, f32)>> {
        let query_emb = self.embedder.embed(query)?;
        let hnsw = self.hnsw.read().await;
        let hnsw = hnsw.as_ref().ok_or_else(|| anyhow::anyhow!("HNSW not built"))?;

        let results = hnsw.search(&query_emb, k);
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
        Self { max_chunk_chars, overlap_chars }
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
