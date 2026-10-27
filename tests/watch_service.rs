//! Mode service `--watch` de bout en bout : démarrage réel, SIGTERM, arrêt
//! propre en 0, et **vidage des événements en attente** au moment du signal.
//!
//! Ignoré par défaut : il exige le modèle BGE-M3 et onnxruntime.
//!
//! ```sh
//! MAIEUTIK_EMBEDDER_MODEL=/chemin/model.onnx \
//! MAIEUTIK_EMBEDDER_TOKENIZER=/chemin/tokenizer.json \
//! ORT_DYLIB_PATH=/chemin/libonnxruntime.so \
//!   cargo test --locked --test watch_service -- --ignored --nocapture
//! ```

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn env_var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} doit être défini pour ce test"))
}

fn log_of(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

#[test]
#[ignore = "exige le modèle BGE-M3 et onnxruntime (voir VERIFICATION.md)"]
fn watch_starts_flushes_pending_event_and_exits_zero_on_sigterm() {
    let model = env_var("MAIEUTIK_EMBEDDER_MODEL");
    let tokenizer = env_var("MAIEUTIK_EMBEDDER_TOKENIZER");
    let ort = env_var("ORT_DYLIB_PATH");

    let sandbox = tempfile::tempdir().unwrap();
    let root = sandbox.path();
    let (work, home, cfg, data) = (
        root.join("work"),
        root.join("home"),
        root.join("cfg"),
        root.join("data"),
    );
    let docs = root.join("docs");
    for d in [&work, &home, &cfg.join("maieutik"), &data, &docs] {
        fs::create_dir_all(d).unwrap();
    }
    fs::write(
        cfg.join("maieutik").join("config.json"),
        format!(
            "{{\"version\":1,\"settings\":{{\"Folders\":[{:?}]}}}}",
            docs.to_str().unwrap()
        ),
    )
    .unwrap();
    fs::write(docs.join("a.md"), "note de depart\n").unwrap();

    let log_path: PathBuf = root.join("service.log");
    let log_file = fs::File::create(&log_path).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_maieutik"))
        .args(["-d", "--watch"])
        .current_dir(&work)
        .env_clear()
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &cfg)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("MAIEUTIK_EMBEDDER_MODEL", &model)
        .env("MAIEUTIK_EMBEDDER_TOKENIZER", &tokenizer)
        .env("ORT_DYLIB_PATH", &ort)
        .stdout(Stdio::null())
        .stderr(Stdio::from(log_file))
        .spawn()
        .expect("lancement de maieutik --watch");

    // Attente de l'annonce du service plutôt qu'un délai arbitraire : le
    // chargement du modèle prend plusieurs secondes.
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        if log_of(&log_path).contains("Surveillance de") {
            break;
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("le service s'est arrêté avant de surveiller : {status:?}\n{}", log_of(&log_path));
        }
        assert!(
            Instant::now() < deadline,
            "service non démarré dans les 180 s :\n{}",
            log_of(&log_path)
        );
        std::thread::sleep(Duration::from_millis(200));
    }

    // Événement créé juste avant le signal : il est encore dans la fenêtre
    // de débounce (2 s) et ne peut être traité que par le vidage d'arrêt.
    fs::write(docs.join("b.md"), "note ajoutee au dernier moment\n").unwrap();

    let killed = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("envoi de SIGTERM");
    assert!(killed.success(), "kill -TERM a échoué");

    let status = child.wait().unwrap();
    let log = log_of(&log_path);
    assert_eq!(status.code(), Some(0), "arrêt sale :\n{log}");
    assert!(log.contains("SIGTERM reçu"), "signal non journalisé :\n{log}");
    assert!(
        log.contains("Arrêt propre terminé"),
        "arrêt non terminé :\n{log}"
    );
    assert!(
        log.contains("b.md") && log.contains("réindexé"),
        "l'événement en attente au moment du signal n'a pas été vidé puis réindexé :\n{log}"
    );
}
