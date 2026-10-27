# Utilisation

Ce guide suppose que Maieutik est installé et configuré (voir [INSTALLATION.md](INSTALLATION.md)). Les exemples utilisent la commande `maieutik` ; si tu n'as pas copié le binaire dans ton `PATH`, remplace-la par `target/release/maieutik`.

## En trois étapes

1. Déclarer le dossier de notes dans `config.json` (clé `settings.Folders`).
2. Indexer : `maieutik index`.
3. Chercher : `maieutik search "ma question"`.

`ORT_DYLIB_PATH` doit être défini dans le terminal où tu lances ces commandes (voir l'installation).

## --watch (mode service)

```sh
maieutik --watch
```

Garde le processus au premier plan et surveille les dossiers de `settings.Folders` : une création, une modification, une suppression ou un renommage déclenche une réindexation du fichier (extension et taille vérifiées, un fichier illisible pour ses métadonnées est ignoré et signalé dans les journaux).

- L'arrêt se fait par `Ctrl-C` (`SIGINT`) ou `SIGTERM`. Les événements détectés moins de 2 secondes avant le signal (fenêtre de regroupement) ne sont pas perdus : ils sont traités avant la sortie, puis le programme sort en 0.
- Le service refuse de démarrer avec le code 4 s'il n'y a aucun dossier configuré, ou si l'index est indisponible (modèle d'embedding absent ou illisible) : il n'y a pas de mode dégradé qui tourne sans jamais pouvoir réindexer.
- `--watch` est un mode, pas une option de commande : `maieutik --watch index` et `maieutik --watch --mcp-stdio` sont des erreurs d'usage (code 2).
- Comme les autres commandes, il crée le verrou d'instance dans le dossier courant : un second `maieutik --watch` sort en 4.
- Au démarrage, l'index est réconcilié : les fichiers supprimés pendant que le service était arrêté en sont retirés, donc ils ne remontent plus dans `search`. Un chemin dont les métadonnées sont illisibles est conservé et signalé : la réconciliation ne supprime rien sur une ambiguïté.
- Un fichier **modifié** pendant l'arrêt est détecté au démarrage suivant et réindexé : l'index garde l'empreinte (taille + date de modification) de chaque version indexée. Un fichier indexé avant l'introduction de l'empreinte est conservé sans être vérifié jusqu'à sa prochaine indexation. Un dossier retiré de `settings.Folders` reste indexé jusqu'à `maieutik index --force`.

## index

```sh
maieutik index
maieutik index --force
```

La commande parcourt les dossiers de `settings.Folders`, découpe chaque fichier retenu en passages, calcule leurs empreintes sémantiques avec le modèle et les enregistre dans l'index local.

- Sans option, les passages s'ajoutent à l'index existant.
- Avec `--force`, chaque fichier trouvé remplace ses passages, puis les fichiers disparus sont retirés : l'index est reconstruit à partir du corpus présent, sans jamais être vidé d'un bloc. Un fichier en échec d'extraction garde son ancien contenu et est compté comme échec.
- Les fichiers dont l'extension n'est pas dans `Extensions`, ou plus gros que `MaxFileSize`, sont ignorés sans message par fichier ; si aucun fichier n'est retenu du tout, un message l'indique et le code est 1.

Sortie :

- stderr : une ligne `échec : chemin : cause` par fichier en erreur, une ligne `sans texte : chemin` par fichier dont aucun texte n'a pu être extrait, et un message si aucun fichier n'est retenu dans des dossiers lisibles (extension ou taille hors limites).
- stdout : le résumé `N indexé(s), M sans texte, K échec(s)`.

Un fichier en échec n'arrête pas les autres.

| Code | Signification |
|---|---|
| 0 | tout est indexé, aucun fichier sans texte, corpus non vide ; ou arrêt propre demandé par `SIGINT`/`SIGTERM` en `--watch` |
| 1 | échec partiel : fichier en échec, fichier sans texte, corpus vide, ou dossier configuré introuvable ou non lisible |
| 2 | erreur d'usage : sous-commande inconnue, `--limit 0`, `--watch` combiné à une sous-commande ou à `--mcp-stdio` |
| 3 | commande non implémentée : chat, model, mcp |
| 4 | environnement indisponible : aucun dossier configuré, aucun dossier lisible, modèle d'embedding absent ou illisible, verrou d'instance non créable, ou une autre instance déjà en cours |

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
- Réindexation automatique partielle : le suivi des fichiers en arrière-plan réindexe les créations, modifications, suppressions et renommages. Un événement survenu pendant un arrêt de Maieutik n'est pas rattrapé : relancer `maieutik index` (ou `--force`) après un arrêt pour réaligner l'index.
- Les PDF sont acceptés par défaut ; l'extraction a été validée sur un corpus réel au P1.17 (chaîne corrigée `lopdf 0.42`), mais pas la chaîne complète indexation + recherche avec le modèle.
- L'option `--config-dir` est acceptée mais reste sans effet.
- Chaque lancement crée dans le dossier courant un petit fichier vide nommé `maieutik` (verrou d'instance unique) ; il peut être supprimé sans risque. Si une autre instance détient déjà le verrou, le programme écrit « Une autre instance de Maieutik est déjà en cours d'exécution. » sur stderr et sort avec le code 4 ; dans un dossier non inscriptible, il écrit « Verrou d'instance indisponible » et sort aussi en 4.
- La taille des passages (1200 caractères, avec 150 de chevauchement) n'a pas été calibrée.

Voir aussi les limites listées dans le [README](../README.md).

