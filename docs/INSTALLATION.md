# Installation

Maieutik n'est pas distribué sous forme de binaire : il se compile depuis les sources. Ce guide décrit une installation sous Linux, documentée sur Arch Linux. Les autres systèmes n'ont pas été essayés.

Une fois le programme installé, voir [UTILISATION.md](UTILISATION.md).

## Vue d'ensemble

1. Installer les outils de compilation.
2. Récupérer les sources et compiler.
3. Installer onnxruntime.
4. Télécharger le modèle d'embedding.
5. Déclarer le dossier de notes dans `config.json`.
6. Vérifier avec `maieutik index`.

Prévoir environ 2,3 Go pour le modèle et plusieurs Go pour la compilation (dossier `target/`).

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

## 5. Configuration

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

## 6. Vérifier

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

## État de vérification

Ce guide a été écrit à partir du code et de sorties d'exécution du programme. L'installation complète n'a pas été rejouée sur une machine neuve. Non rejoués : la compilation release avec `scripts/release.sh`, le téléchargement d'onnxruntime et du modèle sur une machine vierge, et les tests avec modèle (voir [VERIFICATION.md](../VERIFICATION.md)).
