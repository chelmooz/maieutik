//! Mode service `--watch` de bout en bout : démarrage réel, SIGTERM, arrêt
//! propre en 0, vidage des événements en attente, et réconciliation des
//! fichiers disparus pendant l'arrêt.
//!
//! Ignorés par défaut : ils exigent le modèle BGE-M3 et onnxruntime.
//!
//! ```sh
//! MAIEUTIK_EMBEDDER_MODEL=/chemin/model.onnx \
//! MAIEUTIK_EMBEDDER_TOKENIZER=/chemin/tokenizer.json \
//! ORT_DYLIB_PATH=/chemin/libonnxruntime.so \
//!   cargo test --locked --test watch_service -- --ignored --nocapture
//! ```

#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const FOX: &str = "the quick brown fox jumps over the lazy dog near the river";
const RUST: &str = "rust cargo compiler borrow checker lifetimes traits";

fn env_var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} doit être défini pour ce test"))
}

/// Bac à sable isolé : HOME, XDG, config `Folders`, dossier de notes, verrou.
struct Sandbox {
    root: tempfile::TempDir,
}

impl Sandbox {
    fn new(files: &[(&str, &str)]) -> Self {
        let root = tempfile::tempdir().unwrap();
        for dir in ["work", "home", "cfg/maieutik", "data", "cache", "docs"] {
            fs::create_dir_all(root.path().join(dir)).unwrap();
        }
        for (name, body) in files {
            fs::write(root.path().join("docs").join(name), body).unwrap();
        }
        fs::write(
            root.path().join("cfg/maieutik/config.json"),
            format!(
                "{{\"version\":1,\"settings\":{{\"Folders\":[{:?}]}}}}",
                root.path().join("docs").to_str().unwrap()
            ),
        )
        .unwrap();
        Self { root }
    }

    fn docs(&self) -> PathBuf {
        self.root.path().join("docs")
    }

    fn log_path(&self) -> PathBuf {
        self.root.path().join("service.log")
    }

    fn command(&self) -> Command {
        let root = self.root.path();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_maieutik"));
        cmd.current_dir(root.join("work"))
            .env_clear()
            .env("HOME", root.join("home"))
            .env("XDG_CONFIG_HOME", root.join("cfg"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            // Verrou propre au bac à sable : le socket abstrait de
            // `single-instance` 0.3 est global à la machine, et un service
            // laissé tourner ne doit pas faire échouer les tests suivants.
            .env(
                "MAIEUTIK_INSTANCE_NAME",
                format!(
                    "maieutik-test-{}-{}",
                    std::process::id(),
                    root.file_name().unwrap_or_default().to_string_lossy()
                ),
            )
            .env(
                "MAIEUTIK_EMBEDDER_MODEL",
                env_var("MAIEUTIK_EMBEDDER_MODEL"),
            )
            .env(
                "MAIEUTIK_EMBEDDER_TOKENIZER",
                env_var("MAIEUTIK_EMBEDDER_TOKENIZER"),
            )
            .env("ORT_DYLIB_PATH", env_var("ORT_DYLIB_PATH"));
        cmd
    }

    /// Lance `--watch` en journalisant sur `service.log`, puis attend que le
    /// service s'annonce (le chargement du modèle prend plusieurs secondes).
    fn start_watch(&self) -> Service {
        let log = fs::File::create(self.log_path()).unwrap();
        let child = self
            .command()
            .args(["-d", "--watch"])
            .stdout(Stdio::null())
            .stderr(Stdio::from(log))
            .spawn()
            .expect("lancement de maieutik --watch");
        let service = Service {
            child,
            reaped: false,
        };

        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            let log = self.log();
            if log.contains("Surveillance de") {
                return service;
            }
            if log.contains("Réconciliation impossible") || Instant::now() >= deadline {
                panic!("le service n'a pas démarré :\n{log}");
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn log(&self) -> String {
        fs::read_to_string(self.log_path()).unwrap_or_default()
    }
}

/// Service `--watch` en cours. `Drop` garantit qu'aucun processus n'est
/// laissé orphelin, y compris quand un test panique avant l'arrêt.
struct Service {
    child: Child,
    reaped: bool,
}

impl Service {
    /// Envoie SIGTERM puis attend la sortie ; renvoie le code.
    fn stop(mut self) -> i32 {
        let killed = Command::new("kill")
            .args(["-TERM", &self.child.id().to_string()])
            .status()
            .expect("envoi de SIGTERM");
        assert!(killed.success(), "kill -TERM a échoué");
        let status = self.child.wait().unwrap();
        self.reaped = true;
        status.code().expect("sortie par code")
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
#[ignore = "exige le modèle BGE-M3 et onnxruntime (voir VERIFICATION.md)"]
fn watch_starts_flushes_pending_event_and_exits_zero_on_sigterm() {
    let sandbox = Sandbox::new(&[("a.md", "note de depart\n")]);
    let service = sandbox.start_watch();

    // Événement créé juste avant le signal : il est encore dans la fenêtre
    // de débounce (2 s) et ne peut être traité que par le vidage d'arrêt.
    fs::write(
        sandbox.docs().join("b.md"),
        "note ajoutee au dernier moment\n",
    )
    .unwrap();

    let code = service.stop();
    let log = sandbox.log();
    assert_eq!(code, 0, "arrêt sale :\n{log}");
    assert!(
        log.contains("SIGTERM reçu"),
        "signal non journalisé :\n{log}"
    );
    assert!(
        log.contains("Arrêt propre terminé"),
        "arrêt non terminé :\n{log}"
    );
    assert!(
        log.contains("b.md") && log.contains("réindexé"),
        "l'événement en attente au moment du signal n'a pas été vidé puis réindexé :\n{log}"
    );
}

#[test]
#[ignore = "exige le modèle BGE-M3 et onnxruntime (voir VERIFICATION.md)"]
fn watch_startup_purges_files_deleted_while_stopped() {
    let sandbox = Sandbox::new(&[("a.md", FOX), ("b.md", RUST)]);

    let indexed = sandbox.command().arg("index").output().unwrap();
    assert_eq!(
        indexed.status.code(),
        Some(0),
        "index initial : {}",
        String::from_utf8_lossy(&indexed.stderr)
    );

    // Suppression pendant l'arrêt : aucun événement ne sera jamais émis.
    fs::remove_file(sandbox.docs().join("b.md")).unwrap();

    let service = sandbox.start_watch();
    let log = sandbox.log();
    assert!(
        log.contains("Réconciliation : 1 fichier(s) disparu(s)"),
        "la réconciliation de démarrage n'a pas purgé le fichier disparu :\n{log}"
    );
    assert!(
        log.contains("b.md"),
        "fichier purgé non journalisé :\n{log}"
    );

    assert_eq!(service.stop(), 0, "arrêt sale :\n{}", sandbox.log());

    // Preuve visible côté utilisateur : la recherche ne remonte plus b.md.
    let search = sandbox
        .command()
        .args(["search", "rust cargo compiler", "--limit", "3"])
        .output()
        .unwrap();
    assert_eq!(search.status.code(), Some(0));
    let out = stdout_of(&search);
    assert!(
        !out.contains("b.md"),
        "fichier supprimé encore cherchable :\n{out}"
    );
    assert!(
        out.contains("a.md"),
        "le seul fichier restant devrait être trouvé :\n{out}"
    );
}

#[test]
#[ignore = "exige le modèle BGE-M3 et onnxruntime (voir VERIFICATION.md)"]
fn watch_startup_reindexes_files_modified_while_stopped() {
    let sandbox = Sandbox::new(&[("a.md", FOX)]);

    let indexed = sandbox.command().arg("index").output().unwrap();
    assert_eq!(
        indexed.status.code(),
        Some(0),
        "index initial : {}",
        String::from_utf8_lossy(&indexed.stderr)
    );
    let before = sandbox
        .command()
        .args(["search", "quick brown fox", "--limit", "3"])
        .output()
        .unwrap();
    assert_eq!(before.status.code(), Some(0));
    let before = stdout_of(&before);
    assert!(
        before.contains("a.md"),
        "précondition non indexée :\n{before}"
    );

    // Modification pendant l'arrêt : aucun événement ne sera jamais émis.
    std::thread::sleep(std::time::Duration::from_millis(50));
    fs::write(sandbox.docs().join("a.md"), RUST).unwrap();

    let service = sandbox.start_watch();
    let log = sandbox.log();
    assert!(
        log.contains("Réconciliation : 1 fichier(s) modifié(s) pendant l'arrêt réindexé(s)."),
        "la réconciliation n'a pas réindexé le fichier modifié :\n{log}"
    );
    assert!(
        log.contains("a.md"),
        "fichier réindexé non journalisé :\n{log}"
    );

    assert_eq!(service.stop(), 0, "arrêt sale :\n{}", sandbox.log());

    // Preuve visible côté utilisateur : la recherche remonte le NOUVEAU
    // contenu, plus l'ancien.
    let after = sandbox
        .command()
        .args(["search", "quick brown fox", "--limit", "3"])
        .output()
        .unwrap();
    assert_eq!(after.status.code(), Some(0));
    let after = stdout_of(&after);
    assert!(
        !after.contains("lazy dog"),
        "l'ancien contenu est encore cherchable :\n{after}"
    );
    let rust = sandbox
        .command()
        .args(["search", "rust cargo compiler", "--limit", "3"])
        .output()
        .unwrap();
    let rust = stdout_of(&rust);
    assert!(
        rust.contains("borrow checker"),
        "le nouveau contenu n'est pas cherchable :\n{rust}"
    );
}
