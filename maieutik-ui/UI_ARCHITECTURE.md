# maieutik-ui — client local egui (Maieutik · Laya · Odysseus)

Lancer : `cargo run -p maieutik-ui -- --demo` ou
`cargo run -p maieutik-ui -- --laya-bin target/debug/maieutik-mcp --laya-real --mode hybrid`
(`ORT_DYLIB_PATH` requis pour le vrai Laya).

## Liaisons
- Laya : processus fils `maieutik-mcp`, MCP stdio JSON-RPC (health, noul, choice, score).
  Les logs du serveur vont sur **stderr** ; stdout reste le canal MCP.
- Maieutik : socket Unix, trame 4 octets LE + `{ok,result,error}` (pas du JSON-RPC 2.0).
  Méthodes utilisées : `health.check`, `search.semantic`, `search.keyword`, `search.hybrid`.
  `--mode` (défaut `hybrid`) choisit laquelle ; `score` est une **similarité**
  normalisée `[0,1]`, 1 = meilleur.
- Odysseus : sonde de présence seulement (NON CÂBLÉ : aucun contrat UI→Odysseus).
- LLM System 2 : NON CÂBLÉ ; endpoint refusé s'il n'est pas loopback (egress).

## Garanties testées (gates-ui.sh, 6 mutations RED)
eval/ jamais servi · fail-closed sans Noul réel · mock marqué « simulé » ·
recherche non routée = NON CÂBLÉ · bruit stdout compté · egress loopback strict.

`tests/e2e_maieutik.rs` ferme la couture : le client socket de l'UI interroge le
vrai `McpServer::with_state` de Maieutik sur un socket réel, avec un index réel
(`HashEmbedder`) — les trois modes renvoient des hits bien formés.

## Écarts connus (hors crate)
1. Noul simulé : une injection évidente passe (`injection=0.00`). Défaut UI =
   `RequireReal`, donc la requête est stoppée tant que les têtes ONNX ne sont
   pas chargées.
2. Têtes Laya (`noul-head.onnx`, `choice-head.onnx`, `score-head.onnx`) absentes :
   fallbacks heuristiques, `health` répond `*_loaded: false`.
3. Odysseus et la génération System 2 ne sont pas câblés — l'UI l'affiche.
