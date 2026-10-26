# Utilisation

Ce guide suppose que Maieutik est installé et configuré (voir [INSTALLATION.md](INSTALLATION.md)). Les exemples utilisent la commande `maieutik` ; si tu n'as pas copié le binaire dans ton `PATH`, remplace-la par `target/release/maieutik`.

## En trois étapes

1. Déclarer le dossier de notes dans `config.json` (clé `settings.Folders`).
2. Indexer : `maieutik index`.
3. Chercher : `maieutik search "ma question"`.

`ORT_DYLIB_PATH` doit être défini dans le terminal où tu lances ces commandes (voir l'installation).

## index

```sh
maieutik index
maieutik index --force
```

La commande parcourt les dossiers de `settings.Folders`, découpe chaque fichier retenu en passages, calcule leurs empreintes sémantiques avec le modèle et les enregistre dans l'index local.

- Sans option, les passages s'ajoutent à l'index existant.
- Avec `--force`, l'index est vidé puis reconstruit entièrement.
- Les fichiers dont l'extension n'est pas dans `Extensions`, ou plus gros que `MaxFileSize`, sont ignorés sans message par fichier ; si aucun fichier n'est retenu du tout, un message l'indique et le code est 1.

Sortie :

- stderr : une ligne `échec : chemin : cause` par fichier en erreur, une ligne `sans texte : chemin` par fichier dont aucun texte n'a pu être extrait, et un message si aucun fichier n'est retenu dans des dossiers lisibles (extension ou taille hors limites).
- stdout : le résumé `N indexé(s), M sans texte, K échec(s)`.

Un fichier en échec n'arrête pas les autres.

| Code | Signification |
|---|---|
| 0 | tout est indexé, aucun fichier sans texte, corpus non vide |
| 1 | échec partiel : fichier en échec, fichier sans texte, corpus vide, ou dossier configuré introuvable ou non lisible |
| 2 | erreur d'usage : sous-commande inconnue, `--limit 0` |
| 3 | commande non implémentée : chat, model, mcp |
| 4 | index indisponible : aucun dossier configuré, aucun dossier lisible, ou modèle d'embedding absent ou illisible |

## search

```sh
maieutik search "ma question"
maieutik search "ma question" --limit 5
```

`-l` est l'abréviation de `--limit`. Le défaut est 10 résultats.

Chaque ligne de résultat a la forme :

```
<distance> | <chemin du fichier> | <100 premiers caractères du passage>
```

La distance est la distance euclidienne entre la question et le passage, calculée sur des vecteurs normalisés : elle va de 0 à 2, et plus elle est petite, plus le passage est proche de la question. Les résultats sont triés du plus proche au plus éloigné.

Il faut avoir lancé `maieutik index` au moins une fois. Si le modèle d'embedding est absent ou illisible, la commande écrit « Index indisponible » sur stderr, laisse stdout vide et sort en code 4. `--limit 0` est refusé avec le code 2.

## Fichiers et dossiers

| Élément | Emplacement (Linux) |
|---|---|
| Configuration | `$XDG_CONFIG_HOME/maieutik/config.json` (défaut `~/.config/maieutik/config.json`) |
| Modèle | `$XDG_DATA_HOME/maieutik/models/bge-m3/` (défaut `~/.local/share/maieutik/models/bge-m3/`) |
| Index | `$XDG_DATA_HOME/maieutik/vector_index/` |

Pour repartir de zéro, `maieutik index --force` suffit. Supprimer le dossier `vector_index/` a le même effet.

## Journal

L'option `-d` (ou `--debug`) augmente le niveau de détail du journal :

```sh
maieutik -d index
```

## Limites connues

- chat et model sont factices : message sur stderr, code 3.
- Aucune réindexation automatique : après avoir modifié, ajouté ou supprimé des notes, relancer `maieutik index` (ou `--force` pour tout reconstruire). Le suivi des fichiers en arrière-plan est présent mais ne réindexe pas encore les créations ni les modifications.
- Les PDF sont acceptés par défaut mais n'ont pas été vérifiés de bout en bout.
- L'option `--config-dir` est acceptée mais reste sans effet.
- Chaque lancement crée dans le dossier courant un petit fichier vide nommé `maieutik` (verrou d'instance unique). Il peut être supprimé sans risque ; dans un dossier non inscriptible, le programme peut se terminer immédiatement sans message, avec le code 0.
- La taille des passages (1200 caractères, avec 150 de chevauchement) n'a pas été calibrée.

Voir aussi les limites listées dans le [README](../README.md).

