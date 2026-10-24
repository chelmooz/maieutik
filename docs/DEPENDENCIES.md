# Dépendances — inventaire et régime (P1-d)

Méthode : `cargo clean` puis `cargo build --locked --all-targets --timings`
(profil dev, 24 threads). Total mesuré : **2 min 07 s** mur, **32 min 05 s**
CPU utilisateur, 868 unités de build, 546 paquets. « cold » = temps de
compilation du paquet lui-même (max des variantes de features), extrait du
rapport cargo-timings. Coût dominant : `llama-cpp-sys-2` **99,2 s**.

| Crate | Section | Utilisée dans | cold (s) | Note / plan d'élagage |
|---|---|---|---|---|
| `serde` | dependencies | src | 1.1 |  |
| `tokio` | dependencies | src, tests | 11.9 |  |
| `config` | dependencies | src, tests, build.rs | 2.3 |  |
| `notify` | dependencies | src | 2.0 |  |
| `ort` | dependencies | src, tests | 2.6 |  |
| `llama-cpp-2` | dependencies | src | 0.8 |  |
| `sherpa-onnx` | dependencies | UNUSED | 0.4 | liaison statique + téléchargement au build ; réservé P8. GARDER derrière une feature `stt` |
| `reqwest` | dependencies | src, build.rs | 5.5 | default features ON → compile `openssl` (6,9 s) en plus de rustls. `default-features = false` + rustls |
| `jsonrpsee` | dependencies | src | 0.0 |  |
| `comrak` | dependencies | UNUSED | 3.4 | doublon markdown avec `pulldown-cmark` — n’en garder qu’un |
| `tracing-subscriber` | dependencies | src | 3.5 |  |
| `clap` | dependencies | src | 0.1 |  |
| `cpal` | dependencies | UNUSED | 1.2 | feature `jack` impose libjack ; audio inutilisé (voix = voie B, sherpa-onnx). ÉLAGUER ou feature `stt` |
| `uuid` | dependencies | UNUSED | 0.7 | UNUSED confirmé (grep -w). ÉLAGUER |
| `chrono` | dependencies | src | 2.8 |  |
| `pnet` | dependencies | UNUSED | 0.2 | tire `pnet_packet` (6,6 s). UNUSED visible. ÉLAGUER |
| `tokenizers` | dependencies | src | 8.2 |  |

| `anyhow` | dependencies | src | 0.6 |  |
| `thiserror` | dependencies | UNUSED | 0.4 |  |
| `directories` | dependencies | src | 0.2 |  |
| `serde_json` | dependencies | src | 2.3 |  |
| `bincode` | dependencies | src | 0.6 |  |
| `futures` | dependencies | UNUSED | 1.9 |  |
| `once_cell` | dependencies | src | 0.1 |  |
| `parking_lot` | dependencies | UNUSED | 0.6 |  |
| `async-trait` | dependencies | src | 1.4 |  |
| `dirs` | dependencies | src | 0.1 |  |
| `keyring` | dependencies | src | 0.1 |  |
| `walkdir` | dependencies | UNUSED | 0.5 |  |
| `sled` | dependencies | src | 4.2 |  |
| `fast-hnsw` | dependencies | src | 0.7 |  |
| `google-drive3` | dependencies | src | 4.2 | module Drive = code mort (décision : 100 % local) — voir plan |
| `oauth2` | dependencies | src | 1.4 | module Drive = code mort — voir plan |
| `url` | dependencies | build.rs | 1.4 |  |
| `tokio-named-pipes` | dependencies | UNUSED | 0.1 |  |
| `docx-rs` | dependencies | UNUSED | 5.6 |  |
| `lopdf` | dependencies | UNUSED | 7.4 |  |
| `pdf-extract` | dependencies | src | 2.7 |  |
| `zip` | dependencies | src, tests | 1.9 |  |
| `hunspell` | dependencies | UNUSED | 0.1 |  |
| `petgraph` | dependencies | UNUSED | 2.5 |  |
| `pulldown-cmark` | dependencies | UNUSED | 2.5 | UNUSED confirmé — retirer avec `comrak` |
| `tracing` | dependencies | src | 0.3 |  |
| `self_update` | dependencies | UNUSED | 0.9 |  |
| `global-hotkey` | dependencies | UNUSED | 1.0 |  |
| `single-instance` | dependencies | src | 0.1 |  |
| `hound` | dependencies | UNUSED | 0.4 |  |
| `sha2` | dependencies | UNUSED | 1.7 |  |
| `blake3` | dependencies | src | 0.8 |  |
| `base64` | dependencies | UNUSED | 0.7 |  |
| `percent-encoding` | dependencies | UNUSED | 0.1 |  |
| `md5` | dependencies | UNUSED | 0.2 |  |
| `ndarray` | dependencies | src | 5.8 |  |
| `vergen` | build-dependencies | UNUSED | 0.1 | UNUSED (build.rs ne génère plus de version). ÉLAGUER + sa reqwest de build |

## Plan d'élagage proposé (aucun retrait fait dans ce lot)

1. Retraits sans usage visible (à re-vérifier un par un au moment du retrait :
   grep, `cargo check`) : `tokio-named-pipes`, `lopdf`, `hunspell`, `pnet`,
   `md5`, `self_update`, `global-hotkey`, `petgraph`, `vergen` (build-dep) ;
   à arbitrer : `url`, `futures`, `parking_lot`, `uuid`, `base64`,
   `percent-encoding`, `sha2`, les deux parseurs markdown (`comrak`, `pulldown-cmark`)
   helpers de répertoires (`dirs`/`directories`).
1 bis. Retrait du module `src/drive` avec `google-drive3` + `oauth2` (décision
   du propriétaire : 100 % local, pas de Google Drive — code mort planifié
   pour retrait).
2. Reconfiguration : `reqwest` en `default-features = false` + rustls (supprime
   openssl) ; `cpal` sans `jack` (ou retrait avec cpal) ; `hound`/`sherpa-onnx`
   derrière une feature `stt`.
3. Effet attendu : disparition de `tokio 0.1.22` et de la majorité des 14
   advisories RUSTSEC actuels (zip/lopdf/bincode/google-apis-common/failure/
   instant/fxhash/lock_api/memoffset selon les retraits), d'`openssl` et
   d'`onig_sys` ; réduction du temps cold et de la surface d'audit.
   Ces retraits sont hors périmètre P1 (consigne : inventaire seulement) et
   seront exécutés après GO, un retrait par commit, avec cargo audit rejoué.

## Dev-dependencies

| Crate | Utilisée dans | Note |
|---|---|---|
| `tempfile` | tests (répertoires temporaires des tests d'intégration) | gardée |
| `proptest` | prévu P3 (propriétés du chunker) | gardée |
| `pretty_assertions` | UNUSED visible aujourd'hui — à retirer ou utiliser en P3 | à arbitrer |

Le lot P1-d n'a retiré aucune dépendance (consigne).
