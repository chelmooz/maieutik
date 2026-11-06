# 💻 **GUIDE DÉVELOPPEURS - MAIEUTIK**
**Version:** 2.0 | **Date:** 06 Nov 2026 | **Pour:** Équipe Développement

---

## 📡 **1. INTÉGRATION API REST**

### **1.1. Endpoints (Réalité)**

**Base URL :** `http://localhost:3000` (bind 127.0.0.1)

| Méthode | Endpoint | Description |
|---------|----------|-------------|
| GET | `/health` | `{"status":"ok"}` |
| GET | `/api/files` | Liste fichiers indexés |
| GET | `/api/search?query=...` | Recherche hybride RRF (k=20) |
| POST | `/chat` | Chat LLM (Ollama requis) |

> **Pas d'endpoints :** `/api/index`, `/api/stats`, `/api/embed`, `/metrics`
> **Indexation :** via CLI `maieutik index`

---

### **1.2. Exemples d'Appel**

```bash
# Health
curl http://localhost:3000/health

# Fichiers
curl http://localhost:3000/api/files

# Recherche (GET avec query param)
curl "http://localhost:3000/api/search?query=recherche+s%C3%A9mantique"

# Chat (POST JSON)
curl -X POST http://localhost:3000/chat \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Bonjour"}]}'
```

**Réponse recherche :**
```json
{
  "query": "recherche",
  "hits": [
    "/path/file.md: preview text...",
    "/path/other.md: another preview..."
  ]
}
```

**Réponse chat :**
```json
{"message":{"role":"assistant","content":"Réponse du LLM..."}}
```

---

### **1.3. Client Python**

```python
import requests

BASE = "http://localhost:3000"

def health(): return requests.get(f"{BASE}/health").status_code == 200
def files(): return requests.get(f"{BASE}/api/files").json()
def search(q): return requests.get(f"{BASE}/api/search", params={"query": q}).json()
def chat(msg): return requests.post(f"{BASE}/chat", json={"messages":[{"role":"user","content":msg}]}).json()

if health():
    print("Fichiers:", len(files()))
    print("Recherche:", search("test")["hits"][:2])
    print("Chat:", chat("Bonjour")["message"]["content"][:80])
```

---

### **1.4. Client TypeScript (maieutik-web)**

```ts
// maieutik-web/src/api/client.ts
import axios from 'axios';
const API_URL = import.meta.env.VITE_API_URL ?? 'http://localhost:3000';
export const api = axios.create({ baseURL: API_URL });
export const fetchFiles = () => api.get<string[]>('/api/files');
export const searchFiles = (q: string) => api.post('/api/search', { query: q });
// Note: API réelle utilise GET /api/search?query=, pas POST
```

---

## 🔌 **2. INTÉGRATION MCP (Model Context Protocol)**

### **2.1. Démarrage MCP**

```bash
# Stdio (pour OpenCode, Claude Desktop)
maieutik --mcp-stdio

# Socket Unix (défaut /tmp/maieutik-mcp.sock)
MAIEUTIK_MCP_SOCKET=/custom/sock maieutik --mcp-stdio
```

> **Ne pas combiner** avec sous-commande ni `--watch`.

---

### **2.2. Configuration Clients**

**OpenCode** (`.opencode/config.json`) :
```json
{
  "mcpServers": {
    "maieutik": {
      "command": "maieutik",
      "args": ["--mcp-stdio"],
      "env": { "MAIEUTIK_DATA_DIR": "/path/to/data" }
    }
  }
}
```

**Claude Desktop** : Settings → Features → MCP → Add Server
- Command: `maieutik`
- Args: `--mcp-stdio`

---

### **2.3. Méthodes Implémentées**

| Méthode | Description | Statut |
|---------|-------------|--------|
| `health.check` | Health check | ✅ |
| `search.semantic` | Recherche dense HNSW | ✅ |
| `search.keyword` | Recherche BM25 | ✅ |
| `search.hybrid` | Fusion RRF (k=60) | ✅ |
| `files.list/read/write/delete` | Fichiers | ❌ Refusé |
| `chat.complete/stream` | Chat | ❌ Refusé |
| `settings.get/set` | Settings | ❌ Refusé |
| `graph.get` | Graphe | ❌ Refusé |

**Filtres MCP :** `folders` (array), `min_score` (float), `limit` (max 100)

---

## 🦀 **3. SDK RUST (Utilisation comme Lib)**

### **3.1. Cargo.toml**

```toml
[dependencies]
maieutik = { path = "../maieutik" }
tokio = { version = "1.0", features = ["full"] }
anyhow = "1.0"
```

### **3.2. Exemple Complet**

```rust
use maieutik::{
    core::indexer::{VectorIndex, TextEmbedder, HashEmbedder},
    core::retrieval::SearchMode,
    config::store::data_dir,
};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = data_dir()?;
    
    // Embedder: HashEmbedder pour tests, Embedder ONNX pour prod
    let embedder = HashEmbedder::new(); // déterministe
    
    // let embedder = Embedder::new(
    //     Path::new("/path/bge-m3.onnx"),
    //     Path::new("/path/tokenizer.json"),
    // )?;
    
    let index = VectorIndex::open(&data_dir, embedder)?;
    
    // Recherche hybride
    let results = index.search_mode("recherche sémantique", 10, SearchMode::Hybrid).await?;
    
    for hit in results {
        println!("{:.3} | {} | {}", hit.score, hit.file_path, 
            hit.text.chars().take(80).collect::<String>());
    }
    
    // Stats
    let (doc_count, meta) = index.stats();
    println!("Docs: {}, Chunks: {}", doc_count, meta.chunk_count);
    
    Ok(())
}
```

---

### **3.3. Traits d'Extension (Plugins)**

```rust
// Embedder custom
pub trait TextEmbedder: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    fn dim(&self) -> usize;
    fn model_id(&self) -> &str;
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}

// Retriever custom
pub trait Retriever: Send + Sync {
    fn search(&self, query: &str, k: usize) -> Result<Vec<SearchHit>>;
}

// Reranker (P4 - prévu)
pub trait Reranker: Send + Sync {
    fn score_pairs(&self, query: &str, passages: &[String]) -> Result<Vec<f32>>;
}
```

**Exemple plugin embedder :**
```rust
pub struct CustomEmbedder;
impl TextEmbedder for CustomEmbedder {
    fn embed(&self, _: &str) -> Result<Vec<f32>> { Ok(vec![0.0; 1024]) }
    fn dim(&self) -> usize { 1024 }
    fn model_id(&self) -> &str { "custom-v1" }
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> { Ok(vec![vec![0.0; 1024]; texts.len()]) }
}
```

Utilisation :
```rust
let index = VectorIndex::open(&data_dir, CustomEmbedder)?;
```

---

## 🧪 **4. TESTS & QUALITÉ**

### **4.1. Commandes Tests**

```bash
# Tout (workspace)
cargo test --locked --features cpp-modules --workspace

# Unitaires maieutik
cargo test -p maieutik --lib --locked --features cpp-modules

# Tests LLM (nécessite Ollama)
MAIEUTIK_SKIP_OLLAMA=0 cargo test -p maieutik --test llm_ollama --locked --features cpp-modules

# Watcher (ignoré défaut)
cargo test -p maieutik --test watch_service --locked --features cpp-modules

# E2E Web (Playwright)
cd maieutik-web && npm test
```

### **4.2. Validation P0**

```bash
./scripts/validate_p0.sh
# 21/21 checks, score 98/100
```

**Critères P0 :**
- `cargo check --all-targets --all-features` → 0 erreurs
- `cargo clippy` → pas d'erreurs dures (warnings préexistants OK)
- `unwrap()` production ≤ 107 (actuellement 0)
- Tests LLM : ≥ 7 tests, imports corrects (`LlamaManager`, `GenerationParams`)
- Rayon `par_iter` + BM25 incrémental + HNSW rebuild

---

### **4.3. Clippy & Format**

```bash
cargo clippy --locked --features cpp-modules --all-targets
cargo fmt --all
```

**Warnings préexistants connus (non bloquants) :**
- Imports inutilisés dans `embedding_cache.rs`, `registry.rs`
- `cfg(gui)` condition
- `DEFAULT_EMBED_TIMEOUT_SEC` inutilisé
- Type complexe, borrow inutile

---

## 📁 **5. STRUCTURE PROJET (Points Clés)**

```
maieutik/
├── src/
│   ├── main.rs              # CLI: index, search, chat, model, --watch, --mcp-stdio
│   ├── lib.rs               # Re-exports, AppState
│   ├── config/
│   │   ├── mod.rs           # AppSettings (JSON, PascalCase)
│   │   └── store.rs         # config_dir, data_dir, secrets
│   ├── core/
│   │   ├── scan.rs          # Parcours fichiers, chunking, rayon par_iter
│   │   ├── indexer/
│   │   │   ├── mod.rs       # VectorIndex, HNSW, BM25, HashEmbedder, Embedder ONNX
│   │   │   └── bm25.rs      # BM25 incrémental (update_bm25_for_file)
│   │   ├── retrieval/
│   │   │   ├── mod.rs       # SearchMode, Retriever
│   │   │   ├── fusion.rs    # RRF (RRF_K=60)
│   │   │   ├── passage.rs   # Sélection passages (dense_floor=0.25)
│   │   │   └── bm25.rs      # BM25 query
│   │   └── watch.rs         # File watcher (notify), réconciliation démarrage
│   ├── mcp/
│   │   └── mod.rs           # MCP server (stdio + Unix socket), méthodes
│   ├── llm/
│   │   └── ollama.rs        # Client Ollama HTTP
│   ├── model/
│   │   ├── cli.rs           # model list/download/remove/status
│   │   ├── registry.rs      # LocalRegistry (registry.json)
│   │   └── installer.rs     # Téléchargement modèles
│   └── secrets/
│       ├── mod.rs           # SecretsStore trait, backends
│       ├── keyring_store.rs # Keyring backend
│       └── file_store.rs    # File backend (+ encrypt-secrets feature)
├── tests/                   # 36 fichiers de test
├── Cargo.toml               # features: cpp-modules, encrypt-secrets
└── scripts/
    ├── validate_p0.sh       # Validation complète
    └── release.sh           # Build release + strip paths
```

---

## 🔑 **CONSTANTES CLÉS**

| Constante | Valeur | Fichier |
|-----------|--------|---------|
| `EMBEDDING_DIM` | 1024 | `indexer/mod.rs` |
| `MODEL_ID` | `"BAAI/bge-m3"` | `indexer/mod.rs` |
| `CHUNK_MAX_CHARS` | 1200 | `scan.rs` |
| `CHUNK_OVERLAP_CHARS` | 150 | `scan.rs` |
| `RRF_K` | 60 | `retrieval/fusion.rs` |
| `DENSE_FLOOR_SIMILARITY` | 0.25 | `retrieval/passage.rs` |
| `JACCARD_THRESHOLD` | 0.92 | `retrieval/passage.rs` |
| `PASSAGES_PER_FILE` | 2 | `retrieval/passage.rs` |
| `TOKEN_BUDGET` | 1200 | `retrieval/passage.rs` |
| `DEFAULT_EMBED_TIMEOUT_SEC` | 30 | `indexer/mod.rs` |

---

## ✅ **CHECKLIST DÉVELOPPEUR**

- [ ] `cargo check --locked --features cpp-modules --all-targets` passe
- [ ] `cargo test -p maieutik --lib` → 165+ tests OK
- [ ] `./scripts/validate_p0.sh` → 21/21, score 98/100
- [ ] API testée : `/health`, `/api/files`, `/api/search`, `/chat`
- [ ] MCP testé : `maieutik --mcp-stdio` + client
- [ ] Plugins traits compris (`TextEmbedder`, `Retriever`)
- [ ] Documentation `docs/` mise à jour si changements publics