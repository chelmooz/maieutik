# ✅ **CHECKLISTS MAIEUTIK - VALIDATION & VÉRIFICATION**
**Version:** 2.0 | **Date:** 06 Nov 2026

---

## 📋 **1. CHECKLIST INSTALLATION (NOUVEL ENVIRONNEMENT)**

### **✅ Pré-requis Système**
- [ ] OS : Linux (Ubuntu 22.04+), macOS 12+, Windows 10/11 WSL2
- [ ] RAM : ≥ 8 Go (16 Go recommandé pour LLM)
- [ ] CPU : ≥ 4 cœurs (8+ recommandé)
- [ ] Disque : ≥ 50 Go SSD libre (100 Go+ pour gros corpus)
- [ ] Git : ≥ 2.30
- [ ] Docker : ≥ 20.10 (si déploiement Docker)
- [ ] Rust : ≥ 1.99 (si build source)
- [ ] CMake : ≥ 3.18, Clang : ≥ 12, pkg-config (pour `cpp-modules`)

### **✅ Installation Source**
- [ ] `git clone https://github.com/chelmooz/maieutik`
- [ ] `cd maieutik`
- [ ] `cargo build --release --locked --features cpp-modules` → succès
- [ ] Modèles embedding : `maieutik model download bge-m3` → OK
- [ ] LLM : `ollama serve` + `ollama pull qwen2.5:3b` → OK
- [ ] Config : `~/.config/maieutik/config.json` avec `"Folders"` défini
- [ ] Dossiers créés : `~/Documents/notes` (ou chemin config)

### **✅ Première Exécution**
- [ ] `maieutik index` → indexation sans erreur
- [ ] `curl http://localhost:3000/health` → `{"status":"ok"}`
- [ ] `maieutik search --query "test" --k 5` → résultats
- [ ] `maieutik chat --prompt "Bonjour"` → réponse (si Ollama)
- [ ] `maieutik model list` → `bge-m3` installé

### **✅ Post-Install Validation**
- [ ] `maieutik --version` affiche version
- [ ] `maieutik --help` liste commandes : `index`, `search`, `chat`, `model`, `--watch`, `--mcp-stdio`
- [ ] Permissions dossiers notes : `chmod 755 ~/Documents/notes`
- [ ] Watcher test : créer fichier → apparaît dans `/api/files` sous 5s
- [ ] MCP test : `maieutik --mcp-stdio` démarre sans erreur

---

## 📋 **2. CHECKLIST DÉPLOIEMENT PRODUCTION (DEVOPS)**

### **✅ Docker**
- [ ] `docker-compose.yml` présent et configuré
- [ ] Volumes montés :
  - [ ] Notes : `./notes:/data/notes` ou `/data/notes`
  - [ ] Modèles : `./models:/root/.local/share/maieutik/models`
  - [ ] Config : `./config:/root/.config/maieutik`
- [ ] Ports : `3000:3000` (API), `11434:11434` (Ollama si sidecar)
- [ ] Variables d'env définies : `MAIEUTIK_LLM_MODEL`, `MAIEUTIK_INSTANCE_NAME`
- [ ] `docker-compose build` → succès
- [ ] `docker-compose up -d` → conteneurs `Running`
- [ ] Health check : `curl http://localhost:3000/health` → OK

### **✅ Kubernetes (si applicable)**
- [ ] `Deployment` + `Service` + `Ingress` créés
- [ ] `PersistentVolumeClaims` : data (10Gi), models (5Gi), documents (50Gi)
- [ ] `kubectl apply -f` → succès
- [ ] Pods `Running` + `Ready`
- [ ] Liveness/Readiness probes sur `/health` passent
- [ ] Ingress accessible via domaine configuré

### **✅ Reverse Proxy & Sécurité**
- [ ] Nginx/Traefik configuré devant API
- [ ] HTTPS + certificats valides (Let's Encrypt ou certs)
- [ ] Headers sécurité : `X-Frame-Options`, `X-Content-Type-Options`, `HSTS`
- [ ] Rate limiting configuré
- [ ] Auth si public (API Key / JWT middleware)
- [ ] Firewall : seulement 80/443/22 exposés

### **✅ Monitoring & Logs**
- [ ] Logs centralisés (Loki / ELK / Datadog)
- [ ] Health check monitoré (toutes les 30s)
- [ ] Alertes : disque > 80%, RAM > 90%, CPU > 80%, health check down
- [ ] Dashboard Grafana : index size, search latency, embed latency, cache hit rate

### **✅ Backup & Restore**
- [ ] Script backup créé (`backup_maieutik.sh`)
- [ ] Cron quotidien : `0 2 * * * /path/backup_maieutik.sh`
- [ ] Backup inclut : `~/.local/share/maieutik/`, `~/.config/maieutik/`
- [ ] Rétention 30 jours configurée
- [ ] **Restore testé** (mensuel minimum)
- [ ] Procédure restore documentée

### **✅ Mise à jour**
- [ ] Script update créé (`update_maieutik.sh`)
- [ ] Procédure rollback documentée
- [ ] Testé en staging avant prod

---

## 📋 **3. CHECKLIST DÉVELOPPEMENT (CI/CD LOCAL)**

### **✅ Qualité Code**
- [ ] `cargo check --locked --features cpp-modules --all-targets` → 0 erreurs
- [ ] `cargo clippy --locked --features cpp-modules --all-targets` → 0 erreurs dures
- [ ] `cargo fmt --all` → aucun changement
- [ ] `unwrap()` en production = 0 (vérifié par `validate_p0.sh`)

### **✅ Tests**
- [ ] `cargo test -p maieutik --lib --locked --features cpp-modules` → 165+ passed
- [ ] `cargo test --locked --features cpp-modules --workspace` → tout passe
- [ ] Tests LLM : `MAIEUTIK_SKIP_OLLAMA=0 cargo test -p maieutik --test llm_ollama` → 10+ tests
- [ ] Tests watcher : `cargo test -p maieutik --test watch_service` (ignoré défaut)
- [ ] Tests E2E Web : `cd maieutik-web && npm test` → Playwright passe

### **✅ Validation P0 Complète**
- [ ] `./scripts/validate_p0.sh` → **21/21 passed, Score 98/100**

### **✅ Documentation**
- [ ] `docs/` mis à jour si changements API/public
- [ ] `README.md` à jour
- [ ] `CHANGELOG.md` / release notes si release

---

## 📋 **4. CHECKLIST QUOTIDIENNE (OPS / SRE)**

### **✅ Santé Service (Matin)**
- [ ] `curl -f http://localhost:3000/health` → `status: ok`
- [ ] `curl http://localhost:3000/api/files | jq length` → nombre docs cohérent
- [ ] `maieutik search -q test -k 3` → résultats < 200ms
- [ ] Logs : pas d'`ERROR` ni `CRITICAL` depuis dernier check
- [ ] Disque : `df -h` → < 80% utilisé sur volumes data/models
- [ ] RAM : `free -h` → < 85% utilisé
- [ ] CPU : `top -bn1 | head -5` → charge normale

### **✅ Indexation**
- [ ] Nouveaux fichiers apparus dans `/api/files` (si watcher actif)
- [ ] Fichiers modifiés ré-indexés (vérifier timestamp `indexed_at` si dispo)
- [ ] Pas de fichiers "fantômes" (supprimés mais encore dans l'index)

### **✅ Backup**
- [ ] Backup dernière nuit présent dans `/backups/maieutik/`
- [ ] Taille backup cohérente (> 100 MB typiquement)
- [ ] Aucune erreur dans logs backup

---

## 📋 **5. CHECKLIST HEBDOMADAIRE**

### **✅ Maintenance**
- [ ] `git pull origin main` → vérifier changelog
- [ ] `cargo build --release --locked --features cpp-modules` → rebuild si maj
- [ ] `docker-compose pull && docker-compose build --no-cache && docker-compose up -d` (si Docker)
- [ ] `ollama pull qwen2.5:3b` → vérifier maj modèle
- [ ] `maieutik model list` → vérifier versions

### **✅ Performance**
- [ ] Benchmark search : `hyperfine --warmup 3 "maieutik search -q test -k 10"` → < 100ms p95
- [ ] Benchmark index : `time maieutik index --force` (sur échantillon) → temps stable
- [ ] Cache hit rate : `EmbeddingCache::stats()` via script → > 70%

### **✅ Sécurité**
- [ ] `cargo audit` → 0 vulnérabilités critiques
- [ ] Dépendances : `cargo outdated` → maj si critiques
- [ ] Certificats SSL : expiration > 30 jours
- [ ] Secrets rotation si applicable

---

## 📋 **6. CHECKLIST MENSUELLE**

### **✅ Restore Test (CRITIQUE)**
- [ ] Restaurer backup le plus récent en env de test
- [ ] Vérifier : health check, search, chat, nombre docs
- [ ] Documenter durée restore + problèmes éventuels

### **✅ Métriques & Capacity Planning**
- [ ] Croissance index size / mois → projeter 3-6 mois
- [ ] Croissance nombre docs / mois
- [ ] Latence search trend (amélioration/dégradation)
- [ ] Planifier scale (RAM, disque, CPU) si nécessaire

### **✅ Documentation & Formation**
- [ ] FAQ mise à jour avec nouveaux cas support
- [ ] Guides rôle (`docs/guides/`) révisés si changements
- [ ] Session formation équipe si nouvelles features

---

## 📋 **7. CHECKLIST RELEASE (AVANT TAG/PUSH)**

### **✅ Code**
- [ ] `validate_p0.sh` → 21/21, 98/100
- [ ] `cargo test --locked --features cpp-modules --workspace` → tout passe
- [ ] `cargo clippy --locked --features cpp-modules --all-targets` → clean
- [ ] Branche `main` à jour, pas de commits non poussés

### **✅ Build & Artefacts**
- [ ] `scripts/release.sh` → binaires `target/release/maieutik`, `maieutik-api`
- [ ] Binaires testés : `./target/release/maieutik --version`, `maieutik index`, `maieutik search`
- [ ] Docker image build : `docker build -t maieutik:vX.Y.Z .` → succès
- [ ] Docker image testée : `docker run --rm maieutik:vX.Y.Z maieutik --help`

### **✅ Release GitHub**
- [ ] Tag créé : `git tag -a vX.Y.Z -m "Release vX.Y.Z"`
- [ ] `git push origin vX.Y.Z`
- [ ] GitHub Release créé avec :
  - [ ] Changelog
  - [ ] Binaires attachés (Linux x86_64, macOS si dispo)
  - [ ] Checksums SHA256
  - [ ] Notes migration si breaking changes

### **✅ Post-Release**
- [ ] Déploiement staging → validation complète
- [ ] Déploiement prod → validation health check
- [ ] Monitoring : pas de régression latence/erreur
- [ ] Communication équipe : release notes partagées

---

## 📋 **8. CHECKLIST PAR RÔLE (RÉSUMÉ)**

| Rôle | Checklists Principales |
|------|------------------------|
| **DevOps** | #2 Déploiement, #4 Quotidienne, #5 Hebdo, #6 Mensuelle, #7 Release |
| **Développeurs** | #3 Développement, #7 Release |
| **Webdesigners** | `cd maieutik-web && npm run build && npm test` |
| **AI Engineers** | Modèles téléchargés, benchmarks, fine-tuning docs |
| **Data Scientists** | Export embeddings, éval RAG, métriques monitoring |
| **Helpdesk** | FAQ à jour, tickets L1 résolus, escalades suivies |

---

## 🎯 **SCORES DE VALIDATION**

| Checklist | Critère Succès | Score Cible |
|-----------|----------------|-------------|
| **Installation** | Tous items ✅ | 100% |
| **Déploiement Prod** | Tous items ✅ | 100% |
| **Validation P0** | `validate_p0.sh` | 21/21 = 98/100 |
| **Quotidienne** | 0 alertes, health OK | 100% |
| **Hebdo** | Build + benchmarks OK | 100% |
| **Mensuelle** | Restore test réussi | 100% |
| **Release** | Tous gates passent | 100% |

---

## 📝 **TEMPLATE RAPPORT VALIDATION**

```markdown
# Rapport Validation - YYYY-MM-DD

**Environnement:** [Prod / Staging / Dev / Local]
**Version:** `git rev-parse --short HEAD` / `maieutik --version`
**Validé par:** [Nom]

## ✅ Checklist: [Nom checklist]
- [ ] Item 1
- [ ] Item 2
- [x] Item 3 - OK
- [ ] Item 4 - **BLOQUANT**: [détails]

## 📊 Métriques
- Health check: ✅/❌
- Search latency p95: XX ms
- Index size: XXX MB
- Docs count: XXXX
- Cache hit rate: XX%

## ⚠️ Problèmes / Risques
1. [Description]
2. [Description]

## 🎯 Actions Suivantes
- [ ] Action 1 (responsable, échéance)
- [ ] Action 2 (responsable, échéance)

**Statut global:** ✅ VALIDÉ / ⚠️ AVEC RÉSERVES / ❌ REJETÉ
```

---

*Document généré pour Maieutik v2.0 — Maintenu par l'équipe DevOps*