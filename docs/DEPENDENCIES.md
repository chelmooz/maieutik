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
| `jsonrpsee` | dependencies | src | — |  |
| `comrak` | dependencies | UNUSED | 3.4 | doublon markdown avec `pulldown-cmark` — n’en garder qu’un |
| `tracing-subscriber` | dependencies | src | 3.5 |  |
| `clap` | dependencies | src | 0.1 |  |
| `cpal` | dependencies | UNUSED | 1.2 | feature `jack` impose libjack ; audio inutilisé (voix = voie B, sherpa-onnx). ÉLAGUER ou feature `stt` |
| `uuid` | dependencies | UNUSED | 0.7 | UNUSED visible ? à vérifier |
| `chrono` | dependencies | src | 2.8 |  |
| `pnet` | dependencies | UNUSED | 0.2 | tire `pnet_packet` (6,6 s). UNUSED visible. ÉLAGUER |
| `tokenizers` | dependencies | src | 8.2 |  |

## Plan d'élagage proposé (aucun retrait fait dans ce lot)

1. Retraits sans usage visible (à re-vérifier un par un au moment du retrait :
   grep, `cargo check`) : `tokio-named-pipes`, `lopdf`, `hunspell`, `pnet`,
   `md5`, `self_update`, `global-hotkey`, `petgraph`, `vergen` (build-dep) ;
   à arbitrer : `url`, `futures`, `parking_lot`, `uuid`, `base64`,
   `percent-encoding`, `sha2`, un des deux parseurs markdown, un des deux
   helpers de répertoires (`dirs`/`directories`).
2. Reconfiguration : `reqwest` en `default-features = false` + rustls (supprime
   openssl) ; `cpal` sans `jack` (ou retrait avec cpal) ; `hound`/`sherpa-onnx`
   derrière une feature `stt`.
3. Effet attendu : disparition de `tokio 0.1.22` et de la majorité des 14
   advisories RUSTSEC actuels (zip/lopdf/bincode/google-apis-common/failure/
   instant/fxhash/lock_api/memoffset selon les retraits), d'`openssl` et
   d'`onig_sys` ; réduction du temps cold et de la surface d'audit.
   Ces retraits sont hors périmètre P1 (consigne : inventaire seulement) et
   seront exécutés après GO, un retrait par commit, avec cargo audit rejoué.
