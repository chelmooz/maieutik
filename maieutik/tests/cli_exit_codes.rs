//! Tests de contrat CLI : codes de sortie et flux (stdout/stderr) sans modèle.
//!
//! Chaque test lance le binaire dans un répertoire temporaire avec un
//! environnement entièrement contrôlé, sans modèle d'embedding. Le verrou
//! d'instance unique (`single-instance` 0.3, socket abstrait global sous
//! Linux) reçoit un nom propre à chaque bac à sable : les tests s'exécutent en
//! parallèle et ne doivent pas se refuser mutuellement.
//!
//! Ce fichier n'utilise que la bibliothèque standard et `tempfile` : il doit
//! compiler au commit 1, avant toute nouvelle API de `maieutik`.

use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Nom de verrou propre à ce bac à sable : le socket abstrait de
/// `single-instance` est global à la machine, deux tests parallèles qui
/// partageraient « maieutik » se refuseraient mutuellement.
fn instance_name(sandbox: &Path) -> String {
    let leaf = sandbox
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("maieutik-test-{}-{leaf}", std::process::id())
}

/// Lance le binaire dans `sandbox` avec un environnement vidé puis reconstruit.
/// `folders` : dossiers à écrire dans `config.json` (`None` = pas de config).
fn run(sandbox: &Path, folders: Option<&[&Path]>, args: &[&str]) -> Output {
    let work = sandbox.join("work");
    let home = sandbox.join("home");
    let cfg = sandbox.join("cfg");
    let data = sandbox.join("data");
    let cache = sandbox.join("cache");
    for d in [&work, &home, &cfg, &data, &cache] {
        fs::create_dir_all(d).unwrap();
    }
    if let Some(list) = folders {
        let dir = cfg.join("maieutik");
        fs::create_dir_all(&dir).unwrap();
        let json_list: Vec<String> = list
            .iter()
            .map(|p| format!("{:?}", p.to_str().unwrap()))
            .collect();
        let body = format!(
            "{{\"version\":1,\"settings\":{{\"Folders\":[{}]}}}}",
            json_list.join(",")
        );
        fs::write(dir.join("config.json"), body).unwrap();
    }
    Command::new(env!("CARGO_BIN_EXE_maieutik"))
        .args(args)
        .current_dir(&work)
        .env_clear()
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &cfg)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_CACHE_HOME", &cache)
        .env("MAIEUTIK_INSTANCE_NAME", instance_name(sandbox))
        .env("MAIEUTIK_EMBEDDER_MODEL", sandbox.join("absent.onnx"))
        .env("MAIEUTIK_EMBEDDER_TOKENIZER", sandbox.join("absent.json"))
        .output()
        .expect("lancement du binaire maieutik")
}

fn code(o: &Output) -> i32 {
    o.status.code().expect("sortie par code (pas par signal)")
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// Aucun code d'échappement ANSI hors terminal, ni sur stdout ni sur stderr.
fn assert_no_ansi(o: &Output) {
    assert!(!out(o).contains('\u{1b}'), "ANSI sur stdout : {:?}", out(o));
    assert!(!err(o).contains('\u{1b}'), "ANSI sur stderr : {:?}", err(o));
}

/// Cas d'erreur : stdout vide, stderr non vide, aucun ANSI.
fn assert_error_streams(o: &Output) {
    assert_eq!(out(o), "", "stdout doit rester vide sur une erreur");
    assert!(!err(o).trim().is_empty(), "stderr doit expliquer l'erreur");
    assert_no_ansi(o);
}

#[test]
fn help_exits_zero() {
    let t = tempfile::tempdir().unwrap();
    let o = run(t.path(), None, &["--help"]);
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(out(&o).contains("Usage"), "stdout: {}", out(&o));
    assert_no_ansi(&o);
}

#[test]
fn unknown_subcommand_exits_two() {
    let t = tempfile::tempdir().unwrap();
    let o = run(t.path(), None, &["bogus-subcommand"]);
    assert_eq!(code(&o), 2, "stderr: {}", err(&o));
    assert!(!err(&o).trim().is_empty());
}

#[test]
fn search_limit_zero_exits_two() {
    let t = tempfile::tempdir().unwrap();
    let o = run(t.path(), None, &["search", "q", "--limit", "0"]);
    assert_eq!(code(&o), 2, "stdout: {} stderr: {}", out(&o), err(&o));
    assert!(!err(&o).trim().is_empty());
}

#[test]
fn chat_not_implemented_exits_three() {
    let t = tempfile::tempdir().unwrap();
    let o = run(t.path(), None, &["chat", "hello"]);
    assert_eq!(code(&o), 3, "stdout: {} stderr: {}", out(&o), err(&o));
    assert_error_streams(&o);
}

#[test]
fn model_not_implemented_exits_three() {
    let t = tempfile::tempdir().unwrap();
    let o = run(t.path(), None, &["model", "list"]);
    assert_eq!(code(&o), 3, "stdout: {} stderr: {}", out(&o), err(&o));
    assert_error_streams(&o);
}

#[test]
fn mcp_stdio_runs_server_and_exits_on_stdin_close() {
    let t = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_maieutik"));
    let work = t.path().join("work");
    let home = t.path().join("home");
    let cfg = t.path().join("cfg");
    let data = t.path().join("data");
    let cache = t.path().join("cache");
    for d in [&work, &home, &cfg, &data, &cache] {
        fs::create_dir_all(d).unwrap();
    }
    cmd.arg("--mcp-stdio")
        .current_dir(&work)
        .env_clear()
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &cfg)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_CACHE_HOME", &cache)
        .env("MAIEUTIK_INSTANCE_NAME", instance_name(t.path()))
        .env("MAIEUTIK_EMBEDDER_MODEL", t.path().join("absent.onnx"))
        .env("MAIEUTIK_EMBEDDER_TOKENIZER", t.path().join("absent.json"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("lancement maieutik --mcp-stdio");
    // Close stdin immediately to trigger graceful shutdown
    drop(child.stdin.take());
    let o = child.wait_with_output().expect("attente fin");

    // Le serveur doit s'arrêter proprement (stdin fermé = fin de connexion)
    assert_eq!(
        code_from_output(&o),
        0,
        "stdout: {} stderr: {}",
        out(&o),
        err(&o)
    );
}

fn code_from_output(o: &std::process::Output) -> i32 {
    o.status.code().expect("sortie par code")
}

#[test]
fn index_without_folders_exits_four() {
    let t = tempfile::tempdir().unwrap();
    let o = run(t.path(), None, &["index"]);
    assert_eq!(code(&o), 4, "stdout: {} stderr: {}", out(&o), err(&o));
    assert_error_streams(&o);
    assert!(err(&o).contains("Aucun dossier"), "stderr: {}", err(&o));
}

#[test]
fn index_without_model_exits_four() {
    let t = tempfile::tempdir().unwrap();
    let docs = t.path().join("docs");
    fs::create_dir_all(&docs).unwrap();
    let o = run(t.path(), Some(&[docs.as_path()]), &["index"]);
    assert_eq!(code(&o), 4, "stdout: {} stderr: {}", out(&o), err(&o));
    assert_error_streams(&o);
    assert!(
        err(&o).contains("Index indisponible"),
        "stderr: {}",
        err(&o)
    );
}

#[test]
fn search_without_model_exits_four() {
    let t = tempfile::tempdir().unwrap();
    let o = run(t.path(), None, &["search", "q"]);
    assert_eq!(code(&o), 4, "stdout: {} stderr: {}", out(&o), err(&o));
    assert_error_streams(&o);
    assert!(
        err(&o).contains("Index indisponible"),
        "stderr: {}",
        err(&o)
    );
}

#[test]
fn help_mentions_watch_mode() {
    let t = tempfile::tempdir().unwrap();
    let o = run(t.path(), None, &["--help"]);
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(
        out(&o).contains("--watch"),
        "--help doit annoncer le mode service : {}",
        out(&o)
    );
}

/// `--watch` sans dossier configuré : refus immédiat (fail-closed), pas de
/// service fantôme qui tourne sans rien surveiller.
#[test]
fn watch_without_folders_exits_four() {
    let t = tempfile::tempdir().unwrap();
    let o = run(t.path(), None, &["--watch"]);
    assert_eq!(code(&o), 4, "stdout: {} stderr: {}", out(&o), err(&o));
    assert_error_streams(&o);
    assert!(err(&o).contains("Aucun dossier"), "stderr: {}", err(&o));
}

/// `--watch` avec des dossiers mais sans modèle : le service refuse de
/// démarrer plutôt que de tourner en ne pouvant jamais réindexer.
#[test]
fn watch_with_folders_but_no_model_exits_four() {
    let t = tempfile::tempdir().unwrap();
    let docs = t.path().join("docs");
    fs::create_dir_all(&docs).unwrap();
    let o = run(t.path(), Some(&[docs.as_path()]), &["--watch"]);
    assert_eq!(code(&o), 4, "stdout: {} stderr: {}", out(&o), err(&o));
    assert_error_streams(&o);
    assert!(
        err(&o).contains("Index indisponible"),
        "stderr: {}",
        err(&o)
    );
}

/// `--watch` est un mode, pas un modificateur de sous-commande : la
/// combinaison est une erreur d'usage, pas un service qui ignore la commande.
#[test]
fn watch_with_subcommand_exits_two() {
    let t = tempfile::tempdir().unwrap();
    let o = run(t.path(), None, &["--watch", "index"]);
    assert_eq!(code(&o), 2, "stdout: {} stderr: {}", out(&o), err(&o));
    assert_error_streams(&o);
    assert!(err(&o).contains("ne se combine pas"), "stderr: {}", err(&o));
}
