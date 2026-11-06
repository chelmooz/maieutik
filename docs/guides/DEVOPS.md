# 🚀 **GUIDE DEVOPS - MAIEUTIK EN PRODUCTION**
**Version:** 2.0 | **Date:** 06 Nov 2026 | **Pour:** Équipe DevOps

---

## 📦 **1. DÉPLOIEMENT**

### **1.1. Docker (Recommandé)**

**Dockerfile existant** (multi-stage, rust:1.99 builder → debian:bookworm runtime) :
```dockerfile
FROM rust:1.99 as builder
RUN apt-get update && apt-get install -y cmake clang pkg-config && rm -rf /var/lib/apt/lists/*
COPY . /app && WORKDIR /app
RUN cargo build --release --locked --features cpp-modules

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/maieutik /usr/local/bin/
COPY --from=builder /app/target/release/maieutik-api /usr/local/bin/
ENV MAIEUTIK_LLM_MODEL="ollama://qwen2.5:3b"
ENV MAIEUTIK_INSTANCE_NAME="maieutik"
WORKDIR /data
VOLUME ["/data"]
CMD ["maieutik", "index"]
```

**docker-compose.yml existant** :
```yaml
version: "3.8"
services:
  maieutik:
    build: .
    ports: ["3000:3000"]
    volumes:
      - ./notes:/data/notes
      - ./models:/root/.local/share/maieutik/models
      - ./config:/root/.config/maieutik
    environment:
      - MAIEUTIK_LLM_MODEL=ollama://qwen2.5:3b
      - MAIEUTIK_INSTANCE_NAME=maieutik
    restart: unless-stopped
```

> **Note :** `CMD ["maieutik", "index"]` lance l'indexation une fois. Pour service persistant, utiliser `--watch` ou lancer `maieutik-api` séparément.

**Commandes :**
```bash
docker-compose build           # Build
docker-compose up -d           # Démarrer
docker-compose logs -f maieutik # Logs
docker-compose down            # Arrêter
git pull && docker-compose build --no-cache && docker-compose up -d  # Update
```

---

### **1.2. Kubernetes**

**Deployment** (`maieutik-deployment.yaml`) :
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: maieutik
spec:
  replicas: 1
  selector:
    matchLabels:
      app: maieutik
  template:
    metadata:
      labels:
        app: maieutik
    spec:
      containers:
      - name: maieutik
        image: your-registry/maieutik:latest
        env:
        - name: MAIEUTIK_EMBEDDER_MODEL
          value: "/app/models/bge-m3.onnx"
        - name: MAIEUTIK_EMBEDDER_TOKENIZER
          value: "/app/models/tokenizer.json"
        - name: MAIEUTIK_LLM_MODEL
          value: "ollama://qwen2.5:3b"
        - name: RUST_LOG
          value: "info"
        ports:
        - containerPort: 3000
        volumeMounts:
        - name: data; mountPath: /app/data
        - name: models; mountPath: /app/models
        - name: documents; mountPath: /app/documents
        resources:
          limits: {cpu: "4", memory: "8Gi"}
          requests: {cpu: "2", memory: "4Gi"}
        livenessProbe:
          httpGet: {path: /health, port: 3000}
          initialDelaySeconds: 30; periodSeconds: 10
        readinessProbe:
          httpGet: {path: /health, port: 3000}
          initialDelaySeconds: 5; periodSeconds: 5
      volumes:
      - name: data; persistentVolumeClaim: {claimName: maieutik-data}
      - name: models; persistentVolumeClaim: {claimName: maieutik-models}
      - name: documents; persistentVolumeClaim: {claimName: maieutik-documents}
---
apiVersion: v1
kind: Service
metadata: {name: maieutik}
spec:
  selector: {app: maieutik}
  ports: [{protocol: TCP, port: 80, targetPort: 3000}]
  type: LoadBalancer
```

**PVCs** (10Gi data, 5Gi models, 50Gi documents).

---

### **1.3. Binaire Direct**

```bash
cargo build --release --locked --features cpp-modules
./target/release/maieutik --help
# ou
cargo install --path . --features cpp-modules
maieutik --version
```

---

## 🔧 **2. CONFIGURATION**

### **2.1. Variables d'Environnement (Réalité)**

| Variable | Description | Défaut | Requis |
|----------|-------------|--------|--------|
| `MAIEUTIK_CONFIG_DIR` | Config dir | `~/.config/maieutik` | Non |
| `MAIEUTIK_DATA_DIR` | Data dir | `~/.local/share/maieutik` | Non |
| `MAIEUTIK_EMBEDDER_MODEL` | Modèle ONNX path | - | Oui (index) |
| `MAIEUTIK_EMBEDDER_TOKENIZER` | Tokenizer path | - | Oui (index) |
| `MAIEUTIK_EMBEDDER_THREADS` | Threads ONNX | `available_parallelism()` | Non |
| `MAIEUTIK_EMBEDDER_TIMEOUT_SEC` | Timeout batch (s) | 30 | Non |
| `MAIEUTIK_EMBED_CACHE_SIZE` | Cache LRU size | 1000 | Non |
| `MAIEUTIK_LLM_MODEL` | LLM (ex: `ollama://qwen2.5:3b`) | - | Pour `chat` |
| `MAIEUTIK_LLM_CTX_SIZE` | Contexte LLM | - | Non |
| `MAIEUTIK_LLM_N_GPU_LAYERS` | Couches GPU | - | Non |
| `MAIEUTIK_INSTANCE_NAME` | Nom verrou unique | `maieutik` | Non |
| `MAIEUTIK_MCP_SOCKET` | Socket MCP Unix | `/tmp/maieutik-mcp.sock` | Non |
| `MAIEUTIK_SECRETS_BACKEND` | `keyring` \| `file` | `keyring` (fallback file) | Non |
| `RUST_LOG` | Log level | `info` | Non |

> **Pas de `maieutik.toml`** — config persistante en **JSON** (`config.json`, clés PascalCase) dans `MAIEUTIK_CONFIG_DIR`.

---

### **2.2. Config JSON (Exemple)**

```json
// ~/.config/maieutik/config.json
{
  "Extensions": [".txt", ".md", ".docx", ".pdf"],
  "Folders": ["/data/notes"],
  "MaxFileSize": 10,
  "TopMatchesLimit": 10,
  "Theme": "Dark",
  "AiEnabled": true
}
```

---

## 📊 **3. MONITORING & LOGS**

### **3.1. Health Check**

```bash
curl http://localhost:3000/health
# {"status":"ok"}
```

### **3.2. Logs Structurés (tracing + JSON)**

```rust
// Dans main.rs
use tracing_subscriber::{fmt, EnvFilter};
fn init_logging() {
    fmt().with_env_filter(EnvFilter::from_default_env())
        .with_target(true).with_line_number(true).with_file(true)
        .json().init();  // Pour Loki/ELK
}
```

**Exemple log :**
```json
{"timestamp":"2026-11-06T12:00:00.000Z","level":"INFO","target":"maieutik::core::indexer","span":{"name":"embed_batch"},"file":"src/core/indexer/mod.rs","line":214,"message":"Embedding batch of 10 texts","texts_count":10,"duration_ms":456}
```

### **3.3. Centralisation (Loki/ELK)**

```yaml
# promtail-config.yml pour Loki
clients:
  - url: http://loki:3100/loki/api/v1/push
scrape_configs:
  - job_name: docker
    docker_sd_configs:
      - host: unix:///var/run/docker.sock
```

> **Pas d'endpoint `/metrics` Prometheus natif.** Instrumenter custom si besoin.

---

## 🛡️ **4. SÉCURITÉ**

### **4.1. API**

Bind sur `127.0.0.1:3000` (localhost). Exposer via reverse proxy :

```nginx
# /etc/nginx/sites-available/maieutik
server {
    listen 80; server_name maieutik.votre-domaine.com;
    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        client_max_body_size 100M;
    }
    # Security headers
    add_header X-Frame-Options "SAMEORIGIN";
    add_header X-Content-Type-Options "nosniff";
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains";
}
```

**Auth middleware (optionnel) :**
```rust
// maieutik-api/src/middleware.rs
pub async fn api_key_middleware<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let key = req.headers().get("X-API-Key").and_then(|v| v.to_str().ok());
    if key != Some(&std::env::var("MAIEUTIK_API_KEY").unwrap_or_default()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(req).await)
}
```

---

### **4.2. Secrets Backend**

| Backend | Variable | Description |
|---------|----------|-------------|
| `keyring` (défaut) | `MAIEUTIK_SECRETS_BACKEND=keyring` | macOS Keychain, Windows Credential Manager, Linux Secret Service |
| `file` | `MAIEUTIK_SECRETS_BACKEND=file` | Fichier chiffré local (`data/secrets.enc`, feature `encrypt-secrets`) |

---

### **4.3. Modèles**

```bash
# Vérifier checksums
sha256sum ~/.local/share/maieutik/models/bge-m3.onnx
sha256sum ~/.local/share/maieutik/models/tokenizer.json
```

---

### **4.4. Firewall**

```bash
sudo ufw allow 22/tcp   # SSH
sudo ufw allow 80/tcp   # HTTP (Nginx)
sudo ufw allow 443/tcp  # HTTPS (Nginx)
sudo ufw enable
```

---

## 🔄 **5. MISE À JOUR & MAINTENANCE**

### **5.1. Stratégie**

| Type | Fréquence | Méthode |
|------|-----------|---------|
| Patch | Hebdo | `git pull && cargo build --release --locked --features cpp-modules` |
| Minor | Mensuelle | Nouvelle image Docker |
| Major | Trimestrielle | Migration planifiée |

---

### **5.2. Script Update**

```bash
#!/bin/bash
# update_maieutik.sh
set -e
echo "🔄 Mise à jour Maieutik..."
tar -czvf maieutik_backup_$(date +%Y%m%d_%H%M%S).tar.gz ~/.local/share/maieutik/ ~/.config/maieutik/
cd /app && git fetch origin && git checkout main && git pull origin main
cargo build --release --locked --features cpp-modules
if systemctl is-active --quiet maieutik 2>/dev/null; then sudo systemctl restart maieutik; elif [ -f docker-compose.yml ]; then docker-compose down && docker-compose build --no-cache && docker-compose up -d; fi
sleep 5 && curl -f http://localhost:3000/health || exit 1
echo "✅ Terminé !"
```

---

### **5.3. Backup/Restore**

```bash
# backup_maieutik.sh (cron 0 2 * * *)
BACKUP_DIR="/backups/maieutik"; DATE=$(date +%Y%m%d_%H%M%S)
mkdir -p "$BACKUP_DIR"
tar -czvf "$BACKUP_DIR/maieutik_data_$DATE.tar.gz" ~/.local/share/maieutik/ ~/.config/maieutik/
find "$BACKUP_DIR" -name "maieutik_*.tar.gz" -mtime +30 -delete
```

```bash
# restore_maieutik.sh <backup_file>
set -e; BACKUP_FILE=$1
systemctl stop maieutik 2>/dev/null || docker-compose down
tar -xzvf "$BACKUP_FILE" -C /
systemctl start maieutik 2>/dev/null || docker-compose up -d
```

---

## ✅ **CHECKLIST DEVOPS**

- [ ] Docker build + up OK
- [ ] Health check `/health` passe
- [ ] Volumes persistants montés
- [ ] Reverse proxy Nginx + HTTPS
- [ ] Logs centralisés (Loki/ELK)
- [ ] Alertes disque/RAM/CPU
- [ ] Backup quotidien + test restore mensuel
- [ ] Update script testé
- [ ] Firewall configuré
- [ ] Secrets backend choisi (keyring/file)

---

## 📋 **RÉFÉRENCE RAPIDE**

| Commande | Action |
|----------|--------|
| `maieutik index` | Indexer (une fois) |
| `maieutik index --force` | Réindexer tout |
| `maieutik --watch` | Watcher continu |
| `maieutik --mcp-stdio` | Serveur MCP stdio |
| `maieutik search -q "x" -k 10` | Recherche |
| `maieutik chat -p "x" -f file.md` | Chat + contexte |
| `maieutik model list/download/remove/status` | Gestion modèles |
| `curl localhost:3000/health` | Health check |
| `curl localhost:3000/api/files` | Lister fichiers |
| `curl "localhost:3000/api/search?query=x"` | Recherche API |
| `curl -X POST localhost:3000/chat -d '{"messages":[{"role":"user","content":"x"}]}'` | Chat API |