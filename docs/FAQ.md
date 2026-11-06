# ❓ **FAQ UTILISATEUR - MAIEUTIK**
**Version:** 2.0 | **Date:** 06 Nov 2026

---

## 🚀 **INSTALLATION & DÉMARRAGE**

### **Q: Comment installer Maieutik ?**
**R:** Deux options :

**1. Docker (recommandé)**
```bash
git clone https://github.com/chelmooz/maieutik
cd maieutik
docker-compose up -d
# API sur http://localhost:3000
```

**2. Depuis les sources (Linux/macOS/WSL2)**
```bash
git clone https://github.com/chelmooz/maieutik
cd maieutik
# Prérequis: Rust 1.99+, cmake, clang, pkg-config
cargo build --release --locked --features cpp-modules
# Binaire: target/release/maieutik
```

> Voir `docs/INSTALLATION.md` pour le guide complet.

---

### **Q: Quels sont les prérequis système ?**
| Composant | Minimum | Recommandé |
|-----------|---------|------------|
| RAM | 8 Go | 16 Go+ (pour LLM) |
| CPU | 4 cœurs | 8+ cœurs |
| Disque | 50 Go SSD | 100 Go+ |
| OS | Ubuntu 22.04+, macOS 12+, Windows 10 WSL2 | - |
| Rust | 1.99 | 1.99+ |
| Docker | 20.10 | 24+ |

---

### **Q: Comment télécharger les modèles ?**
```bash
# Embedding (BGE-M3)
maieutik model download bge-m3

# LLM via Ollama
ollama serve
ollama pull qwen2.5:3b    # Défaut Docker
ollama pull mistral:7b    # Alternative
```

---

## ⚙️ **CONFIGURATION**

### **Q: Où est le fichier de configuration ?**
**R:** `~/.config/maieutik/config.json` (JSON, clés PascalCase)

```json
{
  "Folders": ["/home/user/Notes"],
  "Extensions": [".md", ".txt", ".pdf", ".docx"],
  "MaxFileSize": 10,
  "TopMatchesLimit": 10
}
```

Variables d'environnement utiles :
```bash
MAIEUTIK_CONFIG_DIR=/custom/config    # Dossier config
MAIEUTIK_DATA_DIR=/custom/data        # Dossier données (index, modèles)
MAIEUTIK_LLM_MODEL=ollama://qwen2.5:3b
```

> **Pas de `maieutik.toml`** — config en JSON uniquement.

---

### **Q: Comment changer le dossier de notes ?**
Éditer `~/.config/maieutik/config.json` :
```json
{ "Folders": ["/nouveau/chemin/notes", "/autre/dossier"] }
```
Puis réindexer : `maieutik index --force`

---

## 📝 **INDEXATION & NOTES**

### **Q: Comment ajouter une note ?**
**Méthode 1 - Fichier direct (auto si watcher) :**
```bash
cat > ~/Notes/ma_note.md << 'EOF'
# Titre
Contenu...
#tags
EOF
```

**Méthode 2 - CLI (recommandé) :**
```bash
maieutik index --path ~/Notes/ma_note.md
maieutik index --path ~/Notes  # Dossier complet
```

**Méthode 3 - Forcer réindexation totale :**
```bash
maieutik index --force
```

---

### **Q: Quels formats de fichiers sont supportés ?**
| Format | Extraction | Statut |
|--------|------------|--------|
| `.md` / `.markdown` | ✅ Complète | Production |
| `.txt` | ✅ Complète | Production |
| `.pdf` | ✅ (lopdf) | Production |
| `.docx` | ✅ | Production |
| Autres | ❌ | Ignorés |

---

### **Q: Comment fonctionne le découpage (chunking) ?**
- **Taille :** 1200 caractères max par chunk
- **Overlap :** 150 caractères entre chunks consécutifs
- **Respecte :** Headers Markdown (`#`, `##`, `###`) pour couper logiquement

---

### **Q: Mes modifications ne sont pas prises en compte ?**
Le **file watcher** (`maieutik --watch`) détecte automatiquement :
- Création → indexe
- Modification → ré-indexe (via fingerprint taille+mtime)
- Suppression → retire de l'index
- Renommage → supprime ancien + indexe nouveau

**Forcer :** `maieutik index --path /chemin/fichier` ou `maieutik index --force`

---

## 🔍 **RECHERCHE**

### **Q: Comment chercher dans mes notes ?**
```bash
# CLI
maieutik search --query "votre recherche" --k 10

# Modes
maieutik search -q "test" --mode semantic  # Vecteurs (sens)
maieutik search -q "test" --mode keyword   # BM25 (mots-clés)
maieutik search -q "test" --mode hybrid    # RRF fusion (défaut, recommandé)

# API
curl "http://localhost:3000/api/search?query=votre+recherche"
```

---

### **Q: Quelle est la différence entre les modes de recherche ?**
| Mode | Technique | Cas d'usage |
|------|-----------|-------------|
| `semantic` | HNSW vecteurs denses | Concepts, synonymes, langage naturel |
| `keyword` | BM25 lexical | Mots exacts, références, codes |
| `hybrid` | RRF (k=60) fusion | **Recommandé** — meilleur des deux |

---

### **Q: Comment filtrer les résultats ?**
**CLI uniquement :**
```bash
maieutik search -q "test" --filter-ext md        # Extension
maieutik search -q "test" --filter-size-lt 10000 # Taille < 10KB
maieutik search -q "test" --filter-size-gt 1000  # Taille > 1KB
```

---

### **Q: La recherche ne retourne rien ?**
1. Vérifier indexation : `curl localhost:3000/api/files`
2. Réindexer : `maieutik index --force`
3. Vérifier dossier config : `cat ~/.config/maieutik/config.json | grep Folders`
4. Vérifier extensions : `.md`, `.txt`, `.pdf`, `.docx` seulement

---

## 💬 **CHAT / LLM**

### **Q: Comment activer le chat ?**
```bash
# Variable requise
export MAIEUTIK_LLM_MODEL=ollama://qwen2.5:3b

# Ollama doit tourner
ollama serve

# Puis
maieutik chat --prompt "Bonjour"
# Ou avec contexte fichier
maieutik chat --prompt "Résume ce document" --file ~/Notes/doc.md
```

**API :**
```bash
curl -X POST http://localhost:3000/chat \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Bonjour"}]}'
```

---

### **Q: Erreur "chat: aucun générateur configuré" ?**
**Cause :** `MAIEUTIK_LLM_MODEL` non définie.
**Fix :**
```bash
export MAIEUTIK_LLM_MODEL=ollama://qwen2.5:3b
# Ou dans ~/.bashrc / ~/.zshrc pour persistance
```

---

### **Q: Erreur "Échec de connexion à Ollama" ?**
1. Ollama tourne ? `ollama serve` ou `systemctl start ollama`
2. Port 11434 accessible ? `curl http://localhost:11434/api/tags`
3. Modèle téléchargé ? `ollama list` → doit montrer `qwen2.5:3b` ou votre modèle

---

### **Q: Quels modèles LLM sont supportés ?**
Via Ollama (recommandé) :
- `qwen2.5:3b` (défaut, rapide, 2-3 Go)
- `mistral:7b` (qualité supérieure, 4-5 Go)
- `llama2:7b`, `phi3:3b`, `codellama:7b`

Via llama.cpp local : **Stub uniquement** (pas encore fonctionnel).

---

## 🔌 **MCP (Model Context Protocol)**

### **Q: C'est quoi MCP ?**
Protocole standard pour connecter des outils IA (OpenCode, Claude, Cursor) à Maieutik.

### **Q: Comment activer MCP ?**
```bash
# Stdio (OpenCode, Claude Desktop)
maieutik --mcp-stdio

# Socket Unix
MAIEUTIK_MCP_SOCKET=/tmp/mon_socket maieutik --mcp-stdio
```

> **Ne pas combiner** avec sous-commande (`index`, `search`, etc.) ni `--watch`.

---

### **Q: Configuration OpenCode ?**
```json
// .opencode/config.json
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

---

### **Q: Configuration Claude Desktop ?**
Settings → Features → MCP → Add Server :
- **Command:** `maieutik`
- **Args:** `--mcp-stdio`

---

### **Q: Quelles fonctions MCP sont disponibles ?**
| Fonction | Description | Statut |
|----------|-------------|--------|
| `health.check` | Santé serveur | ✅ |
| `search.semantic` | Recherche vecteurs | ✅ |
| `search.keyword` | Recherche BM25 | ✅ |
| `search.hybrid` | Fusion RRF | ✅ |
| `files.*` | Gestion fichiers | ❌ Refusé |
| `chat.*` | Chat LLM | ❌ Refusé |
| `settings.*` | Configuration | ❌ Refusé |

---

## 🐳 **DOCKER & DÉPLOIEMENT**

### **Q: Comment déployer en production ?**
```bash
# Docker Compose
docker-compose up -d

# Kubernetes
kubectl apply -f maieutik-deployment.yaml

# Binaire systemd
sudo cp target/release/maieutik /usr/local/bin/
sudo systemctl enable --now maieutik
```

Voir `docs/guides/DEVOPS.md` pour détails complets.

---

### **Q: Ports utilisés ?**
| Service | Port | Description |
|---------|------|-------------|
| API Maieutik | 3000 | REST + MCP (localhost only) |
| Ollama | 11434 | LLM (si sidecar) |
| Frontend React | 5173 | Dev seulement (Vite) |

> API bind sur `127.0.0.1:3000` — utiliser reverse proxy (Nginx) pour exposer.

---

## 🔧 **DÉPANNAGE RAPIDE**

| Problème | Solution Rapide |
|----------|-----------------|
| `Index non trouvé` | `maieutik index` |
| `chat: aucun générateur` | `export MAIEUTIK_LLM_MODEL=ollama://qwen2.5:3b` |
| `Ollama connection failed` | `ollama serve` |
| `Permission denied` | `chmod -R 755 ~/Notes` |
| `Out of memory` | Modèle plus petit (`qwen2.5:1.5b`) |
| `Search vide` | `maieutik index --force` |
| `MCP refused` | `maieutik --mcp-stdio` (sans sous-commande) |
| `Guard rejected` | Éviter "ignore previous instructions", "drop table", etc. |

---

## 📚 **LIENS UTILES**

| Ressource | Lien |
|-----------|------|
| Installation complète | `docs/INSTALLATION.md` |
| Guide utilisation | `docs/UTILISATION.md` |
| Architecture | `docs/ARCHITECTURE_LAYERS.md` |
| Guide DevOps | `docs/guides/DEVOPS.md` |
| Guide Développeurs | `docs/guides/DEVELOPPEURS.md` |
| Guide AI Engineers | `docs/guides/AI_ENGINEERS.md` |
| Guide Data Scientists | `docs/guides/DATA_SCIENTISTS.md` |
| Guide Webdesigners | `docs/guides/WEBDESIGNERS.md` |
| Guide Helpdesk | `docs/guides/HELPDESK.md` |
| RTOC/COT Production | `docs/RTOC_COT_PRODUCTION.md` |
| GitHub Issues | https://github.com/chelmooz/maieutik/issues |
| GitHub Repo | https://github.com/chelmooz/maieutik |

---

## 🆘 **ENCORE BLOQUÉ ?**

1. **Lire les logs :** `journalctl -u maieutik -f` ou `docker logs maieutik`
2. **Vérifier health :** `curl http://localhost:3000/health`
3. **Réindexer :** `maieutik index --force`
4. **Créer un ticket :** GitHub Issues avec :
   - Version (`maieutik --version`)
   - OS / RAM / Disque
   - Logs erreur
   - Étapes reproduction
   - Config anonymisée