//! Contrat : une seconde instance est refusée quand le verrou est détenu.
//!
//! `single-instance` (Linux) verrouille par `flock` un fichier dont le nom est
//! résolu relativement au cwd : le test détient donc le verrou sur le fichier
//! exact que le binaire ouvrira, puis vérifie le refus. Le contrôle négatif
//! vérifie qu'aucun message de second instance n'apparaît sans verrou détenu.

use single_instance::SingleInstance;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn code(o: &Output) -> i32 {
    o.status.code().expect("sortie par code (pas par signal)")
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// Lance le binaire dans un bac à sable (environnement vidé puis reconstruit,
/// même contrat que tests/cli_exit_codes.rs).
fn run(sandbox: &Path, args: &[&str]) -> Output {
    let work = sandbox.join("work");
    let home = sandbox.join("home");
    let cfg = sandbox.join("cfg");
    let data = sandbox.join("data");
    let cache = sandbox.join("cache");
    for d in [&work, &home, &cfg, &data, &cache] {
        fs::create_dir_all(d).unwrap();
    }
    Command::new(env!("CARGO_BIN_EXE_maieutik"))
        .args(args)
        .current_dir(&work)
        .env_clear()
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &cfg)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_CACHE_HOME", &cache)
        .env("MAIEUTIK_EMBEDDER_MODEL", sandbox.join("absent.onnx"))
        .env("MAIEUTIK_EMBEDDER_TOKENIZER", sandbox.join("absent.json"))
        .output()
        .expect("lancement du binaire maieutik")
}

#[test]
fn second_instance_is_refused_when_lock_is_held() {
    let sandbox = tempfile::tempdir().unwrap();
    let work = sandbox.path().join("work");
    fs::create_dir_all(&work).unwrap();

    // Le nom du verrou du binaire (« maieutik ») est résolu relativement à son
    // cwd : on détient le verrou sur ce fichier exact.
    let lock_path = work.join("maieutik");
    let lock = SingleInstance::new(lock_path.to_str().unwrap()).unwrap();
    assert!(lock.is_single(), "précondition : le test détient le verrou");

    let out = run(sandbox.path(), &["index"]);
    assert_eq!(
        code(&out),
        4,
        "une seconde instance doit sortir en 4 ; stderr : {}",
        err(&out)
    );
    assert!(
        err(&out).contains("déjà en cours d'exécution"),
        "stderr doit expliquer le refus ; obtenu : {}",
        err(&out)
    );
    assert!(
        out.stdout.is_empty(),
        "stdout doit rester vide ; obtenu : {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn first_instance_is_not_refused() {
    // Contrôle négatif : sans verrou détenu, le binaire franchit le verrou
    // (il échoue ensuite pour une raison sans rapport : pas de dossier).
    let sandbox = tempfile::tempdir().unwrap();
    let out = run(sandbox.path(), &["index"]);
    assert_ne!(code(&out), 0, "sans modèle ni dossier, la sortie est non nulle");
    assert!(
        !err(&out).contains("déjà en cours d'exécution"),
        "aucun message de second instance sans verrou détenu ; obtenu : {}",
        err(&out)
    );
}
