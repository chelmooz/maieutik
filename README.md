# Maieutik

[![Rust](https://img.shields.io/badge/rustc-1.99.1%2B-blue?logo=rust)](https://blog.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![MCP](https://img.shields.io/badge/MCP-MCP-green?logo=rocket)](https://github.com/modelcontextprotocol/rust-sdk)

> Maieutik est un moteur de prise de notes local avec IA. Il permet d'indexer vos notes, de les retrouver par similarité sémantique, et de générer des réponses sourcées. Tout s'exécute localement.

---

## 📚 Table des matières

- [Vue d'ensemble](#-vue-densemble)
- [Installation](#-installation)
- [Démarrage rapide](#-démarrage-rapide)
- [Architecture](#-architecture)
- [Commandes principales](#-commandes-principales)
- [Statut du projet](#-statut-du-projet)
- [Prérequis](#-prérequis)
- [Tests](#-tests)
- [Limites connues](#-limites-connues)
- [Licence](#-licence)

---

## 🚀 Vue d'ensemble

Maieutik (grâce à sa manière de penser, inspirée par Socrate) est un système de prise de notes **local-first** qui combine :

- **Indexation RAG** : Recherche par similarité sémantique avec embeddings BGE-M3
- **Gestion de modèles** : CLI pour lister, télécharger, et gérer les modèles LLM
- **MCP Bridge** : Pont JSON-RPC natif pour intégration avec d'autres systèmes
- **Chat avec Ollama** : Génération de réponses via backend Ollama local

**Architecture hybride** : Maieutik (moteur RAG) + Laya (System 1) + maieutik-ui (client egui) + Odysseus (hors dépôt) + Omarchy (environnement dédié).

---

## 📦 Installation

Voir le guide complet dans [maieutik/docs/INSTALLATION.md](maieutik/docs/INSTALLATION.md).

```sh
git clone https://github.com/chelmooz/maieutik
cd maieutik
scripts/release.sh
```

### Configuration

1. Installez onnxruntime 1.31.0
2. Téléchargez le modèle BGE-M3 ONNX (model.onnx, model.onnx_data, tokenizer.json)
3. Configurez votre dossier de notes dans `~/.config/maieutik/config.json`

---

## 🏃 Démarrage rapide

```sh
# Indexer vos notes
maieutik index

# Rechercher dans vos notes
maieutik search "ma question"

# Utiliser le chat avec Ollama
MAIEUTIK_LLM_MODEL="ollama://qwen2.5:7b" maieutik chat "Explique RAG"
```

---

## 🧩 Architecture

```mermaid
flowchart TD
    %% Core components
    Omarchy["Omarchy<br/>Arch/Hyprland + 2 Docker"]
    Laya["Laya (System 1)<br/>noul / choice / score"]
    Odysseus["Odysseus<br/>Agentic Cockpit"]
    Maieutik["Maieutik<br/>moteur RAG"]
    MCP["Pont MCP<br/>Unix socket + stdio"]
    UI["maieutik-ui (egui)"]
    
    %% P0 Additions
    SecretsStore["Secrets Store<br/>+ keyring support"]
    ModelCLI["model CLI<br/>list/download/remove/status"]
    
    %% P3 Additions
    Ollama["Ollama<br/>LLM Backend (qwen2.5:7b)"]
    ChatModule["chat.complete<br/>MCP method"]
    
    %% Connections
    Omarchy -->|"Docker #1"| Laya
    Omarchy -->|"Docker #2"| Odysseus
    UI -->|"MCP stdio|<br/>Score/Noul"| Laya
    UI -->|"search/MCP"| MCP
    MCP -->|"index, search, chat.complete"| Maieutik
    ChatModule -->|"Ollama API"| Ollama
    Odysseus -.->|"client MCP"| MCP
    
    classDef wired stroke:#16a34a,stroke-width:2px;
    classDef planned stroke:#9ca3af,stroke-width:1px,stroke-dasharray: 5 5;
    class Maieutik,MCP,UI,Laya,SecretsStore,ModelCLI,ChatModule,Ollama wired;
    class Odysseus,Omarchy planned;
```

---

## ⌨️ Commandes principales

```sh
# Indexer les dossiers configurés
maieutik index [--force]

# Rechercher
maieutik search "requête" [--limit 10]

# Mode service : surveille les dossiers jusqu'à SIGINT/SIGTERM
maieutik --watch

# Model management (P0)
maieutik model list
maieutik model download <nom>
maieutik model remove <nom> [--force]
maieutik model status <nom>

# Chat avec Ollama (P3)
MAIEUTIK_LLM_MODEL="ollama://qwen2.5:7b" maieutik chat "Quel est le concept de RAG ?"
```

---

## 📊 Statut du projet

| Élément | Statut | Tests |
|---------|--------|-------|
| Secrets Store | ✅ Implémenté | 7 tests |
| Model CLI | ✅ Implémenté | 6 tests |
| Download Manager | ✅ Implémenté | 6 tests |
| Model Installer | ✅ Implémenté | 3 tests |
| Indexation BGE-M3 | ✅ Fonctionnel | Intégration réelle |
| Recherche HNSW/BM25 | ✅ Fonctionnel | Tests P2 |
| Chat Ollama CLI | ✅ Fonctionnel | Testé en bout |
| Chat MCP (P3) | ✅ Fonctionnel | Testé JSON-RPC |

---

## 📋 Prérequis

- Rust 1.99.1+
- onnxruntime 1.31.0
- Modèle BGE-M3 ONNX (2.27 GB)
- Ollama (optionnel, pour le chat)

---

## 🧪 Tests

```sh
# Sans modèle
cargo test --locked

# Avec le modèle réel
cargo test --locked -- --ignored --nocapture --test-threads=1
```

---

## ⚠️ Limites connues

- Le modèle BGE-M3 fait 2.27 GB pour `model.onnx_data`
- La recherche hybride utilise RRF (k=60) sans recalibrage fin
- Le verrou d'instance est global au poste (socket abstrait)

---

## 📄 Licence

- **Licence** : MIT (`LICENSE`)
- **Dépôt** : https://github.com/chelmooz/maieutik
- **Avis tiers** : `THIRD_PARTY_NOTICES.md`

---

*Guide d'utilisation détaillé* : [maieutik/docs/UTILISATION.md](maieutik/docs/UTILISATION.md) | [Installation complète](maieutik/docs/INSTALLATION.md)