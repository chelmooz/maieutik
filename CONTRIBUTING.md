# Contribuer à Maieutik

Merci de contribuer. Le projet est en **alpha** : l'API, le format d'index et les
commandes peuvent changer sans préavis avant la 1.0.

## Prérequis

- Rust 1.75+
- onnxruntime 1.31.0 (voir [`maieutik/docs/INSTALLATION.md`](maieutik/docs/INSTALLATION.md))
- Le modèle BGE-M3 ONNX pour les tests d'intégration
- Ollama (optionnel, pour le chat)

## Mise en place

```sh
git clone https://github.com/chelmooz/maieutik
cd maieutik
cargo build --workspace
```

## Avant d'ouvrir une pull request

```sh
cargo fmt --all
cargo clippy -p maieutik -- -D warnings
cargo test -p maieutik
cargo test -p laya-primitives
cargo test -p maieutik-ui
bash gates-ui.sh
```

`cargo test --workspace` peut dépasser les limites de temps de certains
environnements : lancez les tests par crate.

## Règles de code

- Pas de `unwrap()` ni d'`expect()` dans le code de production.
- Un test par comportement ajouté (TDD).
- Le code et les identifiants sont en anglais ; les messages d'erreur et l'UI
  sont en français.
- Ne jamais committer de secrets, de modèles ONNX, de données personnelles ou
  de fichiers générés.
- Mettre à jour [`VERIFICATION.md`](VERIFICATION.md) quand un contrat change
  (CLI, MCP, format d'index, codes de sortie).

## Commits

Conventional Commits : `feat:`, `fix:`, `docs:`, `test:`, `refactor:`,
`chore:`. Un commit doit correspondre à un changement cohérent.

## Signaler un bug

Ouvrez une issue avec :

- la version (`maieutik --version`) et l'OS ;
- la commande exacte et sa sortie complète ;
- si possible, un corpus minimal qui reproduit le problème.

## Licence

En contribuant, vous acceptez que votre contribution soit publiée sous licence
MIT (voir [`LICENSE`](LICENSE)).
