# Comparaison : Maieutik vs Karpathy LLM Wiki

## Vue d'ensemble

| Aspect | Maieutik | Karpathy LLM Wiki (plugin Obsidian) |
|--------|----------|-------------------------------------|
| **Type** | Moteur RAG en Rust avec MCP | Plugin Obsidian en TypeScript |
| **Mode de fonctionnement** | Hybrid Search : HNSW + BM25 à la volée | Wiki persisté, LLM maintient les pages |
| **Stockage** | Index vectoriel + index BM25 | Markdown dans un dossier (index par LLM) |
| **Retrieval** | Embeddings BGE-M3 + recherche rapide | Graph-based retrieval (Personalized PageRank) |
| **Langages** | Rust (backend) + egui (UI) | TypeScript (plugin) |
| **Déploiement** | Binaire autonome ou via Docker | Plugin Obsidian (desktop + mobile) |
| **Licence** | MIT | Apache-2.0 |
| **Écosystème LLM** | Ollama local | 16+ providers (Anthropic, OpenAI, Ollama, etc.) |
| **Fonctionnalités** | RAG hybride, MCP, gestion de modèles | Ingest PDF/Office, graph view, lint, Q&A |

## Comment se différencier

### Maieutik : RAG dynamique

- **Recherche à la demande** : chaque requête déclenche un embedding + recherche HNSW/BM25
- **Index en temps réel** : les documents sont indexés quand vous ajoutez/modifiez vos notes
- **MCP natif** : exposer un serveur JSON-RPC pour intégration avec Claude, Odysseus, etc.
- **Local-first** : tout s'exécute localement, pas de backend externe

### Karpathy LLM Wiki : Wiki persisté

- **Wiki compilé** : le LLM écrit et maintient un wiki Markdown persistant
- **Ingest différé** : le LLM intègre les sources, crée des pages d'entité et de concept
- **Query vs Build** : le LLM retrouve les pages pertinentes et répond de façon synthétique
- **Local-first** : aucune dépendance backend, tout dans votre vault Obsidian

## Quand choisir l'un ou l'autre

### Choisir Maieutik quand :

- Vous avez besoin d'une **recherche ultra-rapide** sur un corpus volumineux
- Vous voulez un **MCP serveur** pour connecter plusieurs agents
- Vous préférez un **binaire autonome** sans Obsidian
- Vous avez besoin d'une **CLI puissante** intégrable dans vos scripts
- Vous voulez **BGE-M3 ONNX local** sans API externe

### Choisir Karpathy LLM Wiki quand :

- Vous utilisez déjà **Obsidian** comme centre de vos notes
- Vous préférez un **wiki qui grandit avec l'interactions LLM**
- Vous avez besoin de **PDF/Office ingest** natif
- Vous voulez profiter de la **bibliographie 11-langues** du plugin
- Vous avez besoin de **Q&A dans l'éditeur** avec citations

## Intégration possible

Les deux systèmes sont compatibles :

- Maieutik peut servir de **backend RAG** au plugin Karpathy Wiki
- Le MCP de Maieutik peut être consommé par des agents externes tout en utilisant le wiki Obsidian comme source
- On peut utiliser Karpathy Wiki pour le **logging et la synthèse** tout en utilisant Maieutik pour le **chat en temps réel**

## Références

- [Karpathy LLM Wiki gist](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)
- [Plugin Obsidian karpathywiki](https://community.obsidian.md/plugins/karpathywiki)
- [Plugin Obsidian community-plugins.json](https://github.com/obsidianmd/obsidian-releases/blob/master/community-plugins.json)