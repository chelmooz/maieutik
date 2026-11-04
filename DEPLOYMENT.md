# Déploiement Maieutik

Ce guide explique comment déployer Maieutik en production.

## 📦 Binaires pré-compilés

Les binaires sont créés automatiquement par GitHub Actions sur chaque release.

- **maieutik** : Moteur RAG principal
- **maieutik-ui** : Interface graphique (egui)

## 🚀 Déploiement rapide

### Option 1: Binaires pré-compilés (recommandé)

```bash
# 1. Télécharger la dernière release
wget https://github.com/chelmooz/maieutik/releases/latest/download/maieutik-linux-x86_64.tar.gz
tar -xzf maieutik-linux-x86_64.tar.gz
sudo cp maieutik /usr/local/bin/
sudo cp maieutik-ui /usr/local/bin/

# 2. Vérifier l'installation
maieutik --version
```

### Option 2: Compilation depuis les sources

```bash
# Prérequis
sudo apt install -y rustc cargo cmake clang pkg-config libssl-dev

# Clone
git clone https://github.com/chelmooz/maieutik.git
cd maieutik

# Build
cargo build --release -p maieutik -p maieutik-ui

# Les binaires seront dans target/release/
```

## ⚙️ Configuration

### Variables d'environnement

```bash
# Required for BGE-M3 embedding
export ORT_DYLIB_PATH="/usr/lib/libonnxruntime.so"

# Optional: LLM for chat
export MAIEUTIK_LLM_MODEL="ollama://qwen2.5:7b"

# Optional: Instance isolation
export MAIEUTIK_INSTANCE_NAME="work-notes"
```

### Fichier de configuration (`~/.config/maieutik/config.json`)

```json
{
  "version": 1,
  "settings": {
    "Folders": [
      "/home/user/documents/notes"
    ],
    "Extensions": [".md", ".txt", ".docx", ".pdf"],
    "MaxFileSize": 10
  }
}
```

## 🐳 Déploiement avec Docker (recommandé pour production)

### Docker Compose

```yaml
version: '3.8'

services:
  maieutik:
    image: ghcr.io/chelmooz/maieutik:latest
    volumes:
      - ./notes:/data/notes
      - ./config:/root/.config/maieutik
      - ./models:/root/.local/share/maieutik/models
      - ./index:/root/.local/share/maieutik/vector_index
    environment:
      - ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so
      - MAIEUTIK_LLM_MODEL=ollama://qwen2.5:7b
    ports:
      - "8080:8080"
```

## 📊 Monitoring

### Logs

```bash
# Voir les logs en temps réel
journalctl -u maieutik -f

# Ou avec le binaire directement
maieutik --watch > /var/log/maieutik.log 2>&1
```

### Health check

```bash
# Vérifier que le serveur répond
curl -s http://localhost:8080/health | jq
```

## 🔧 Maintenance

### Réindexer

```bash
maieutik index --force
```

### Mettre à jour

```bash
# Si utilisant Docker
docker pull ghcr.io/chelmooz/maieutik:latest
docker restart maieutik

# Si utilisant les binaires
# Télécharger la nouvelle release et remplacer les fichiers
```

## 🆘 Dépannage

Voir [INSTALLATION.md](maieutik/docs/INSTALLATION.md) pour les problèmes courants.