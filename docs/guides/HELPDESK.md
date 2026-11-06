# 🆘 **GUIDE HELPDESK - MAIEUTIK SUPPORT UTILISATEURS**
**Version:** 2.0 | **Date:** 06 Nov 2026 | **Pour:** Équipe Helpdesk / Support

---

## 📞 **1. NIVEAUX DE SUPPORT (SLA)**

| Niveau | Équipe | Réponse | Résolution | Exemples |
|--------|--------|---------|------------|----------|
| **L1 - Helpdesk** | Support | < 1h | < 4h | Install, config, usage basique |
| **L2 - Développeurs** | Dev | < 4h | < 24h | Bugs, perf, intégration API |
| **L3 - DevOps/AI** | Spécialistes | < 8h | < 48h | Modèles, infra, données |
| **L4 - Core Team** | Maintainers | < 24h | < 72h | Bugs critiques, features |

---

## 🎫 **2. TEMPLATE TICKET**

```markdown
# 🎫 Ticket Support - Maieutik

**ID:** [AUTO - ex: MAI-2026-001]
**Date:** YYYY-MM-DD HH:MM
**Priorité:** [🔴 Urgent / 🟡 Haute / 🟢 Moyenne / 🔵 Basse]
**Statut:** [⏳ Ouvert / 🟡 En cours / ✅ Résolu / ❌ Fermé / 🔄 En attente client]

## 👤 Utilisateur
- **Nom / Email:**
- **Version Maieutik:** `maieutik --version` ou `git rev-parse --short HEAD`
- **OS:** [Ubuntu 22.04 / macOS 14 / Windows 11 WSL2]
- **Arch:** [x86_64 / arm64]
- **RAM / CPU / Dispo disque:**
- **Modèles installés:** `maieutik model list`
- **Config LLM:** `echo $MAIEUTIK_LLM_MODEL`

## 🐛 Problème
[Description claire et concise]

## 🔍 Étapes Reproduction
1. 
2. 
3. 

## 📝 Logs / Erreurs
```
[Coller logs pertinents - journalctl, docker logs, stderr CLI]
```

## ✅ Actions Tentées
- [ ] Redémarrer service (`systemctl restart maieutik` / `docker restart maieutik`)
- [ ] Vérifier permissions dossiers (`ls -la ~/Documents/notes`)
- [ ] Vérifier espace disque (`df -h`)
- [ ] Vérifier Ollama (`curl http://localhost:11434/api/tags`)
- [ ] Réindexer (`maieutik index --force`)
- [ ] Autre: 

## 📎 Pièces Jointes
- [ ] Screenshot erreur
- [ ] Fichier config (`~/.config/maieutik/config.json`)
- [ ] Logs complets (dernières 100 lignes)
- [ ] Export diagnostic (si script dispo)
```

---

## ❓ **3. FAQ UTILISATEUR (Réponses Prêtes)**

### **📁 GÉNÉRAL**

#### **Q: Où mettre mes notes ?**
**R:** Dans les dossiers configurés dans `~/.config/maieutik/config.json` → clé `"Folders"`. Par défaut : `~/Documents/notes` ou `/data/notes` (Docker).
```json
{ "Folders": ["/home/user/Notes", "/home/user/Projets/docs"] }
```
✅ Noms descriptifs (`reunion_2026-11-06.md`)  
✅ Extensions : `.md`, `.txt`, `.pdf`, `.docx`  
✅ Éviter >1000 fichiers par dossier

---

#### **Q: Comment savoir si l'indexation a marché ?**
**R:**
```bash
# API
curl http://localhost:3000/api/files
# → ["path/file1.md", "path/file2.md"]

# CLI
maieutik search --query "test" --k 5

# Logs
journalctl -u maieutik -f | grep -i index
# ou
docker logs maieutik | grep -i index
```

---

#### **Q: Le frontend marche sans backend ?**
**R:** **Non.** Le frontend (React/egui) appelle l'API REST (`/api/search`, `/chat`) et le CLI pour l'indexation.
**Alternatives :**
- API directe : `curl "http://localhost:3000/api/search?query=test"`
- MCP : `maieutik --mcp-stdio` (OpenCode, Claude Desktop)
- CLI : `maieutik search --query "test" --k 10`

---

#### **Q: Comment vérifier que tout va bien ?**
**R:**
```bash
# Health check
curl http://localhost:3000/health
# → {"status":"ok"}

# Si KO
journalctl -u maieutik -f
# ou
docker logs maieutik

# Ressources
free -h && df -h && top -bn1 | head -20
```

---

#### **Q: Comment fonctionne l'indexation ?**
**R:** Processus automatique :
1. **Extraction** : Lecture fichier → texte UTF-8 nettoyé
2. **Chunking** : 1200 caractères + overlap 150 (respecte headers Markdown)
3. **Embedding** : BGE-M3 (1024D) → normalisation L2 → compression f16
4. **Index HNSW** : Vecteurs denses pour recherche sémantique
5. **Index BM25** : Index lexical (k1=1.2, b=0.75) pour mots-clés
6. **Stockage** : sled DB (tables `vectors`, `meta`, `file_meta`)

**Temps estimé :** 100 docs ~30s, 1k docs ~2-4 min, 10k docs ~20-40 min

---

#### **Q: Comment ajouter une note ?**
**R:** 3 méthodes :

**1. Fichier direct (auto si `--watch`)**
```bash
cat > ~/Documents/notes/ma_note.md << 'EOF'
# Ma Note
## Tags
#maieutik #projet
EOF
```

**2. CLI (recommandé)**
```bash
maieutik index --path ~/Documents/notes/ma_note.md
maieutik index --path ~/Documents/notes  # Dossier complet
maieutik index --force  # Tout réindexer
```

**3. Via API** — *Pas d'endpoint `/api/index`*. Utiliser CLI.

**Métadonnées utiles (front matter) :**
```markdown
---
title: Réunion équipe
author: Jean Dupont
date: 2026-11-06
tags: [reunion, équipe, projet]
---
```

---

#### **Q: Intégration Odysseus ?**
**R:** **Oui, via MCP.** Odysseus est **client** MCP, il appelle Maieutik.
```yaml
# Côté Odysseus
mcp_servers:
  maieutik:
    command: "maieutik"
    args: ["--mcp-stdio"]
```
Maieutik n'appelle **pas** Odysseus (pas de contrat UI→Odysseus).

---

#### **Q: Comment faire une recherche avancée ?**
**R:** Via CLI (API limitée à `?query=`) :
```bash
# Modes
maieutik search --query "recherche" --mode semantic --k 10   # Vecteurs
maieutik search --query "recherche" --mode keyword --k 10    # BM25
maieutik search --query "recherche" --mode hybrid --k 10     # RRF (défaut)

# Filtres (CLI seulement)
maieutik search --query "test" --filter-ext md --filter-size-lt 10000

# Chat avec contexte fichier
maieutik chat --prompt "Résume" --file ~/Documents/notes/reunion.md
```

---

#### **Q: Comment mettre à jour un document déjà indexé ?**
**R:** Automatique avec **file watcher** (`maieutik --watch`).
1. Watcher détecte modif (taille + mtime)
2. Calcule fingerprint (hash taille+mtime)
3. Si changé → supprime anciens chunks + ré-indexe

**Forcer :**
```bash
maieutik index --path ~/Documents/note_modifiee.md
maieutik index --force  # Tout
```

---

### **🔧 DÉPANNAGE COURANT**

| Symptôme | Cause Probable | Solution |
|----------|----------------|----------|
| `Index non trouvé` | Pas indexé | `maieutik index` |
| `chat: aucun générateur` | `MAIEUTIK_LLM_MODEL` absent | `export MAIEUTIK_LLM_MODEL=ollama://qwen2.5:3b` |
| `Ollama connection failed` | Ollama pas démarré | `ollama serve` |
| `Permission denied` | Dossiers non lisibles | `chmod -R 755 ~/Documents/notes` |
| `Out of memory` | Modèle trop gros / RAM faible | Modèle plus petit (`qwen2.5:1.5b`) |
| `Search returns empty` | Index vide / mauvais chemin | `maieutik index --force` |
| `MCP connection refused` | Mauvais args / socket | `maieutik --mcp-stdio` (sans sous-commande) |
| `Error: guard rejected` | Query contient patterns interdits | Éviter "ignore previous instructions", "drop table", etc. |

---

### **📋 VÉRIFICATIONS PREMIER NIVEAU (L1)**

Avant d'escalader, vérifier :
1. ✅ `maieutik --version` → version connue
2. ✅ `curl localhost:3000/health` → `{"status":"ok"}`
3. ✅ `curl localhost:3000/api/files` → liste non vide
4. ✅ `maieutik search -q test -k 5` → résultats
5. ✅ `maieutik model list` → `bge-m3` présent
6. ✅ `ollama list` → modèle LLM présent (si chat)
7. ✅ Espace disque > 2 Go (`df -h`)
8. ✅ Permissions dossiers notes OK
9. ✅ Pas d'erreurs `ERROR` dans logs récents
10. ✅ Redémarrage service tenté

---

## 📚 **4. RÉSOURCES DOCUMENTATION**

| Doc | Lien | Public |
|-----|------|--------|
| Installation | `docs/INSTALLATION.md` | Utilisateurs |
| Utilisation | `docs/UTILISATION.md` | Utilisateurs |
| Architecture | `docs/ARCHITECTURE_LAYERS.md` | Tech |
| RTOC/COT Prod | `docs/RTOC_COT_PRODUCTION.md` | Équipe complète |
| Guide DevOps | `docs/guides/DEVOPS.md` | DevOps |
| Guide Dev | `docs/guides/DEVELOPPEURS.md` | Dev |
| Guide AI | `docs/guides/AI_ENGINEERS.md` | AI Engineers |
| Guide Data | `docs/guides/DATA_SCIENTISTS.md` | Data Scientists |
| Guide Webdesign | `docs/guides/WEBDESIGNERS.md` | Webdesigners |

---

## 🚨 **5. PROCÉDURES ESCALADE**

### **L1 → L2 (Dev)**
- Bug reproductible avec steps clairs
- Erreur dans logs applicatifs (pas infra)
- Performance anormale (search > 500ms)
- API retourne 500 / erreur inattendue

### **L2 → L3 (DevOps/AI)**
- Problème modèle (embedding/LLM)
- Infrastructure (Docker, K8s, volumes)
- Optimisation perf (indexation, mémoire)
- Données corrompues / index incohérent

### **L3 → L4 (Core)**
- Regression critique (build cassé, tests fail)
- Security vulnerability
- Feature request architecture
- Release blocking issue

**Info à fournir à chaque escalade :**
- Ticket ID + historique actions
- Logs complets (dernières 200 lignes)
- Config anonymisée
- Steps reproductions minimaux
- Environnement exact

---

## ✅ **CHECKLIST HELPDESK QUOTIDIEN**

- [ ] Tickets nouveaux triés (priorité + assignation)
- [ ] Tickets L1 résolus / fermés
- [ ] Escalades L2/L3 suivies
- [ ] FAQ mise à jour si nouveau cas fréquent
- [ ] Métriques : temps réponse moyen, taux résolution L1
- [ ] Backup vérifié (dernier < 24h)
- [ ] Health check monitoring OK