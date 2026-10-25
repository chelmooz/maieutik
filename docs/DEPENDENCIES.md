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
| `ort` | dependencies | src, tests | 2.6 | features réduites (S17) : `default-features = false`, sans `tls-native`/`download-binaries`/`copy-dylibs` — openssl-sorti du graphe |
| `llama-cpp-2` | dependencies | src | 0.8 |  |
| `sherpa-onnx` | dependencies | UNUSED | 0.4 | liaison statique + téléchargement au build ; réservé P8. **fait (P1.5-15)** : `optional = true`, feature `stt` |
| `reqwest` | dependencies, build-dependencies | src, build.rs | 5.5 | **fait (P1.5-13)** : `default-features = false` sur les deux déclarations, `rustls-tls` ajouté au build-dep — plus d'openssl via reqwest |
| `jsonrpsee` | dependencies | src | 0.0 |  |
| `comrak` | dependencies | UNUSED | 3.4 | **RETIRÉ (P1.5-S3)** |
| `tracing-subscriber` | dependencies | src | 3.5 |  |
| `clap` | dependencies | src | 0.1 |  |
| `cpal` | dependencies | UNUSED | 1.2 | **RETIRÉ (P1.5-S14)** |
| `uuid` | dependencies | UNUSED | 0.7 | UNUSED confirmé (grep -w). ÉLAGUER |
| `chrono` | dependencies | src | 2.8 |  |
| `pnet` | dependencies | UNUSED | 0.2 | **RETIRÉ (P1.5-S7)** |
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
| `google-drive3` | dependencies | src | 4.2 | **RETIRÉ (P1.5-S1)** |
| `oauth2` | dependencies | src | 1.4 | **RETIRÉ (P1.5-S1)** |
| `url` | dependencies | build.rs | 1.4 | **RETIRÉ (P1.6-2)** |
| `tokio-named-pipes` | dependencies | UNUSED | 0.1 | **RETIRÉ (P1.5-S2)** |
| `docx-rs` | dependencies | UNUSED | 5.6 | **RETIRÉ (P1.11-3)** |
| `lopdf` | dependencies | UNUSED | 7.4 | **RETIRÉ (P1.5-S5)** |
| `pdf-extract` | dependencies | src | 2.7 |  |
| `zip` | dependencies | src, tests | 1.9 |  |
| `hunspell` | dependencies | UNUSED | 0.1 | **RETIRÉ (P1.5-S6)** |
| `petgraph` | dependencies | UNUSED | 2.5 | **RETIRÉ (P1.5-S9)** |
| `pulldown-cmark` | dependencies | UNUSED | 2.5 | **RETIRÉ (P1.5-S4)** |
| `tracing` | dependencies | src | 0.3 |  |
| `self_update` | dependencies | UNUSED | 0.9 | **RETIRÉ (P1.5-S10)** |
| `global-hotkey` | dependencies | UNUSED | 1.0 | **RETIRÉ (P1.5-S11)** |
| `single-instance` | dependencies | src | 0.1 |  |
| `hound` | dependencies | UNUSED | 0.4 |  |
| `sha2` | dependencies | UNUSED | 1.7 |  |
| `blake3` | dependencies | src | 0.8 |  |
| `base64` | dependencies | UNUSED | 0.7 |  |
| `percent-encoding` | dependencies | UNUSED | 0.1 |  |
| `md5` | dependencies | UNUSED | 0.2 | **RETIRÉ (P1.5-S8)** |
| `ndarray` | dependencies | src | 5.8 |  |
| `vergen` | build-dependencies | UNUSED | 0.1 | **RETIRÉ (P1.5-S12)** |

| `tempfile` | dev-dependencies | tests | 0.7 |  |
| `proptest` | dev-dependencies | UNUSED | 3.6 | utilisé à partir de P3 (propriétés du chunker) |
| `pretty_assertions` | dev-dependencies | UNUSED | 0.3 | UNUSED confirmé — à retirer ou utiliser en P3 |

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


Le lot P1-d n'a retiré aucune dépendance (consigne).
