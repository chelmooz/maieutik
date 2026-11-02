# Installation

Maieutik n'est pas distribué sous forme de binaire : il se compile depuis les sources. Ce guide décrit une installation sous Linux, documentée sur Arch Linux. Les autres systèmes n'ont pas été essayés.

**Architecture hybride** : Maieutik (moteur RAG) + **Odysseus** (orchestration agentique, MCP) + **Laya** (System 1 local) + **Omarchy** (Arch Linux / Hyprland + 2 Docker dédiés).

Une fois le programme installé, voir [UTILISATION.md](UTILISATION.md).

## Vue d'ensemble

1. Installer les outils de compilation.
2. Récupérer les sources et compiler Maieutik.
3. Installer onnxruntime.
4. Télécharger le modèle d'embedding BGE-M3.
5. **Installer Odysseus** (optionnel, pour orchestration agentique).
6. **Configurer Laya MCP Server** (optionnel, pour System 1 primitives).
7. **Configurer Omarchy + 2 Docker** (optionnel, pour déploiement GPU).
8. Déclarer le dossier de notes dans `config.json`.
9. Vérifier avec `maieutik index`.

Prévoir environ 2,3 Go pour le modèle BGE-M3 et plusieurs Go pour la compilation (dossier `target/`).

## 1. Outils de compilation

- Rust 1.99.1 (version testée ; `Cargo.toml` exige au moins la 1.99), par exemple avec [rustup](https://rustup.rs).
- `cmake`, `clang` et `pkg-config`, depuis les paquets de ta distribution.

## 2. Sources et compilation

```sh
git clone https://github.com/chelmooz/maieutik
cd maieutik
scripts/release.sh
```

Le binaire est produit dans `target/release/maieutik`. Le script lance `cargo build --release --locked` en remplaçant dans le binaire les chemins personnels (dossier de l'utilisateur, dossier du projet) ; voir [scripts/release.sh](../scripts/release.sh).

La compilation release est longue : le profil utilise `lto = "fat"`, et le crate `llama-cpp-sys-2` est le plus lent à compiler (environ 99 s sur la machine de développement, d'après [DEPENDENCIES.md](DEPENDENCIES.md)).

Pour un essai rapide, `cargo build --locked` produit un binaire de développement dans `target/debug/maieutik`, plus gros et moins optimisé.

## 3. onnxruntime

Maieutik charge onnxruntime dynamiquement. La version testée est la 1.31.0.

1. Télécharger l'archive Linux x64 (CPU) de la [version 1.31.0](https://github.com/microsoft/onnxruntime/releases/tag/v1.31.0).
2. La décompresser où tu veux, par exemple dans `~/outils/onnxruntime-1.31.0`.
3. Indiquer la bibliothèque avec `ORT_DYLIB_PATH`, variable requise avec le chargement dynamique :

```sh
export ORT_DYLIB_PATH="$HOME/outils/onnxruntime-1.31.0/lib/libonnxruntime.so"
```

## 4. Modèle d'embedding BGE-M3

Maieutik utilise le modèle [BAAI/bge-m3](https://huggingface.co/BAAI/bge-m3) (licence MIT d'après sa page), au format ONNX, en précision fp32. Seule la sortie dense (1024 dimensions) est utilisée.

Dans le dossier [onnx/](https://huggingface.co/BAAI/bge-m3/tree/main/onnx) du dépôt du modèle, télécharger :

- `model.onnx` (environ 725 ko) ;
- `model.onnx_data` (environ 2,27 Go) ;
- `tokenizer.json` (environ 17 Mo).

`model.onnx` et `model.onnx_data` doivent rester dans le même dossier.

Emplacement par défaut sous Linux : `$XDG_DATA_HOME/maieutik/models/bge-m3/`, soit `~/.local/share/maieutik/models/bge-m3/` si `XDG_DATA_HOME` n'est pas défini. Ce dossier doit contenir `model.onnx` et `tokenizer.json`. Pour un autre emplacement, définir `MAIEUTIK_EMBEDDER_MODEL` (chemin de `model.onnx`) et `MAIEUTIK_EMBEDDER_TOKENIZER` (chemin de `tokenizer.json`).

Les empreintes SHA-256 attendues des trois fichiers sont dans [VERIFICATION.md](../VERIFICATION.md) ; les comparer avec la sortie de `sha256sum`.

Le dossier `onnx/` du modèle contient aussi un petit fichier, `Constant_7_attr__value`, absent de la copie de développement. Si le chargement du modèle échoue, le télécharger aussi et le placer à côté de `model.onnx`.

Variables d'environnement reconnues :

| Variable | Rôle |
|---|---|
| `ORT_DYLIB_PATH` | chemin de `libonnxruntime.so` |
| `MAIEUTIK_EMBEDDER_MODEL` | chemin de `model.onnx` |
| `MAIEUTIK_EMBEDDER_TOKENIZER` | chemin de `tokenizer.json` |
| `MAIEUTIK_EMBEDDER_THREADS` | nombre de threads de calcul (défaut : parallélisme de la machine) |
| `MAIEUTIK_INSTANCE_NAME` | nom du verrou d'instance unique (défaut : `maieutik`) ; à changer pour isoler deux bacs à sable sur le même poste |
| `MAIEUTIK_LLM_MODEL` | modèle LLM pour le chat : `ollama://nom_modele` ou chemin GGUF local (défaut : non défini) |

## 5. Odysseus (Optionnel — Orchestration Agentique)

Odysseus fournit l'interface agente (MCP Client Host, Autonomous Loop, Deep Research, Sandbox Bash/Python).

```sh
git clone https://github.com/odysseus-ai/odysseus  # ou le fork local
cd odysseus
# Installation Python (venv recommandé)
python -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
# Copier .env.example vers .env et configurer LLM_HOST, OLLAMA_BASE_URL, etc.
cp .env.example .env
# Lancer l'app (UI) ou le serveur MCP RAG
python app.py              # Interface web
python mcp_servers/rag_server.py  # MCP Server pour Maieutik
```

Variables d'environnement utiles pour l'intégration Maieutik :

| Variable | Rôle |
|---|---|
| `PYTHONPATH` | Doit inclure le chemin racine d'Odysseus (ex: `/home/chelmooz/Projects/odysseus`) |
| `LLM_HOST` | Hôte LLM (défaut: localhost) |
| `OLLAMA_BASE_URL` | URL Ollama (ex: `http://host.docker.internal:11434/v1`) |

Le serveur MCP `rag_server.py` expose les outils de recherche pour Maieutik via le protocole MCP.

## 6. Laya MCP Server (Optionnel — System 1 Primitives)

Laya expose les primitives `Choice` (routing), `Score` (RAG critic), `Noul` (guardrail) via MCP stdio.

```sh
cd /home/chelmooz/Work/maieutik-corrige/laya-primitives
# Build (nécessite Rust, onnxruntime, modèle ModernBERT)
cargo build --release --bin maieutik-mcp
# Wrapper pour opencode MCP
export RUST_LOG=info
./maieutik-mcp-wrapper.sh
```

Le wrapper `maieutik-mcp-wrapper.sh` configure `RUST_LOG=info` et lance le binaire avec le bon `Cargo.toml`.

Configuration opencode (`~/.config/opencode/opencode.json` ou `opencode.json` du projet) :

```json
{
  "mcp": {
    "servers": {
      "odysseus-rag": {
        "type": "local",
        "command": ["python", "/home/chelmooz/Projects/odysseus/mcp_servers/rag_server.py"],
        "environment": {"PYTHONPATH": "/home/chelmooz/Projects/odysseus"}
      },
      "maieutik-laya": {
        "type": "local",
        "command": ["/home/chelmooz/Work/maieutik-corrige/laya-primitives/maieutik-mcp-wrapper.sh"]
      }
    }
  }
}
```

## 7. Omarchy + 2 Docker Dédiés (Optionnel — Déploiement GPU)

L'hôte Omarchy (Arch Linux / Hyprland) isole le calcul dans 2 conteneurs Docker :

- **Docker #1** : Laya + MCP Bridge (CPU, léger)
- **Docker #2** : Ollama/vLLM + bge-m3 (GPU, NVIDIA Container Toolkit requis)

```sh
# Sur l'hôte Omarchy
cd /home/chelmooz/Projects/odysseus
docker-compose up -d  # ou docker-compose.gpu-nvidia.yml pour GPU
```

Voir `docker-compose.yml` et `docker-compose.gpu-nvidia.yml` dans le dépôt Odysseus.

## 8. Interface graphique (`maieutik-ui`)

Le client egui est un membre du workspace ; rien à installer de plus que la
toolchain et les bibliothèques X11/Wayland.

```sh
cargo build -p maieutik-ui                       # GUI
cargo build -p maieutik-ui --no-default-features # headless (tests, CI)

# Démo, tout simulé et marqué comme tel
cargo run -p maieutik-ui -- --demo

# Réel : Laya en processus fils MCP + Maieutik par socket Unix
ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so.1.31.0 \
  cargo run -p maieutik-ui -- --laya-bin target/debug/maieutik-mcp --laya-real --mode hybrid

# Pont Odysseus : connexion UI → Odysseus (bearer token, scope chat)
# Le contrat est défini dans maieutik-ui/contracts/odysseus-bridge.schema.json
# Pour activer : définir ODYSSEUS_ROOT et exporter le token API
#   export ODYSSEUS_ROOT=/home/user/Projects/odysseus
#   # Générer un token via l'API admin d'Odysseus ou via `odysseus token create`
#   export ODYSSEUS_API_TOKEN=votre_token_ici
#   # L'UI chargera automatiquement le pont ; test : Route → Odysseus → recherche RAG
cargo run -p maieutik-ui -- --odysseus $ODYSSEUS_ROOT --llm-url http://localhost:11434/v1
```

Le garde-fou par défaut est `RequireReal` : sans têtes Laya réelles, la requête
est arrêtée (fail-closed) et l'UI l'affiche. `--mode semantic|keyword|hybrid`
choisit la méthode de recherche du pont (défaut `hybrid`).

## 9. Configuration

Au premier lancement, Maieutik crée ses dossiers de configuration et de données, mais pas le fichier de configuration. Il faut créer `config.json` :

- sous Linux : `$XDG_CONFIG_HOME/maieutik/config.json`, soit `~/.config/maieutik/config.json` si `XDG_CONFIG_HOME` n'est pas défini.

Contenu minimal :

```json
{
  "version": 1,
  "settings": {
    "Folders": ["/home/toi/notes"]
  }
}
```

Les clés du bloc `settings` sont en PascalCase :

| Clé | Rôle | Défaut |
|---|---|---|
| `Folders` | liste des dossiers à indexer (chemins absolus conseillés) | liste vide |
| `Extensions` | extensions retenues, avec le point | `[".txt", ".md", ".docx", ".pdf"]` |
| `MaxFileSize` | taille maximale d'un fichier, en Mo ; au-delà, le fichier est ignoré sans message | `10` |

Les autres clés (interface, raccourcis) ont des valeurs par défaut et peuvent être omises.

## 10. Vérifier

```sh
export ORT_DYLIB_PATH="$HOME/outils/onnxruntime-1.31.0/lib/libonnxruntime.so"
target/release/maieutik index
```

La commande affiche une ligne de bilan de la forme `N fichier(s) indexé(s), M échec(s)`. Pendant le chargement du modèle, le journal affiche de nombreuses lignes d'onnxruntime (« Fused reshape node… ») : elles sont normales. Les codes de sortie sont décrits dans [UTILISATION.md](UTILISATION.md#index).

## Problèmes courants

| Symptôme | Cause probable | Que faire |
|---|---|---|
| « Index indisponible : modèle d'embedding introuvable ou illisible » (code 2) | `model.onnx` ou `tokenizer.json` absent à l'emplacement attendu, ou modèle qui ne s'initialise pas | Lire le journal juste au-dessus : « Embedding model/tokenizer not found » donne les deux chemins attendus ; « Failed to initialize vector index » signale un échec de chargement (vérifier `ORT_DYLIB_PATH` et les fichiers du modèle) |
| « Aucun dossier à indexer » (code 2) | `settings.Folders` vide ou absent de `config.json` (le message parle de `Folders`) | Renseigner `settings.Folders` |
| « échec : chemin : No such file or directory » (code 1) | un dossier de `Folders` n'existe pas | Corriger le chemin |
| « échec : chemin : invalid Zip archive » (code 1) | un fichier `.docx` est corrompu ou n'est pas un vrai fichier Word | Réexporter le fichier |
| Rien n'est indexé et le programme se termine sans message | un fichier est plus gros que `MaxFileSize`, ou son extension n'est pas dans `Extensions` | Ajuster `MaxFileSize` ou `Extensions` |

## 11. Utiliser le chat avec Ollama (P3)

Le chat avec Ollama est une fonctionnalité clé de P3. Voici comment l'utiliser :

### Prérequis

1. **Ollama installé et fonctionnel** :
```sh
# Installation Ollama (Linux)
curl -fsSL https://ollama.com/install.sh | sh
```

2. **Modèle LLM chargé** (par défaut qwen2.5:7b) :
```sh
ollama pull qwen2.5:7b
ollama run qwen2.5:7b  # Pour vérifier que le modèle fonctionne
```

### Configuration

Définir la variable d'environnement `MAIEUTIK_LLM_MODEL` :

```sh
# Pour Ollama
export MAIEUTIK_LLM_MODEL="ollama://qwen2.5:7b"

# Ajoutez cette ligne dans votre ~/.bashrc ou ~/.zshrc
echo 'export MAIEUTIK_LLM_MODEL="ollama://qwen2.5:7b"' >> ~/.bashrc
source ~/.bashrc
```

### Utilisation CLI

```sh
# Chat interactif
maieutik chat "Quel est le concept de RAG ?"

# Chat avec message
maieutik chat "Explique le LLM"
```

### Utilisation via MCP (chat.complete)

Le serveur MCP expose également la méthode `chat.complete` :

```sh
# Via stdio
echo '{"jsonrpc":"2.0","id":1,"method":"chat.complete","params":{"message":"Bonjour"}}' | maieutik --mcp-stdio

# Via socket Unix
# Le socket se trouve généralement dans /run/user/$(id -u)/maieutik_mcp.sock
```

### Modes de modèles supportés

- `ollama://qwen2.5:7b` - Qwen2.5 7B via Ollama
- `ollama://llama3.2:3b` - Llama 3.2 3B via Ollama
- `ollama://<votre-modele>` - Tout modèle installé via Ollama

## 12. État de vérification

Ce guide a été écrit à partir du code et de sorties d'exécution du programme. L'installation complète n'a pas été rejouée sur une machine neuve. Non rejoués : la compilation release avec `scripts/release.sh`, le téléchargement d'onnxruntime et du modèle sur une machine vierge, et les tests avec modèle (voir [VERIFICATION.md](../VERIFICATION.md)).
