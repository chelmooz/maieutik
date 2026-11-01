//! Contrat : une seconde instance est refusée quand le verrou est détenu.
//!
//! `single-instance` 0.3 (Linux) lie un socket abstrait dont le nom est global
//! au réseau — le verrou n'est plus un fichier relatif au cwd comme en 0.1.
//! Deux bacs à sable qui partageraient le nom par défaut « maieutik » se
//! refuseraient donc mutuellement, et un `maieutik --watch` déjà lancé ferait
//! échouer la suite. Chaque test rend le nom unique via
//! `MAIEUTIK_INSTANCE_NAME`, détient le verrou sous ce nom exact, puis vérifie
//! le refus. Le contrôle négatif vérifie qu'aucun message de second instance
//! n'apparaît sans verrou détenu.

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

/// Nom de verrou propre au processus de test et au test courant : le socket
/// abstrait est global à la machine, un nom partagé ferait échouer les tests
/// voisins.
fn instance_name(test: &str) -> String {
    format!("maieutik-test-{}-{test}", std::process::id())
}

/// Lance le binaire dans un bac à sable (environnement vidé puis reconstruit,
/// même contrat que tests/cli_exit_codes.rs).
fn run(sandbox: &Path, instance: &str, args: &[&str]) -> Output {
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
        .env("MAIEUTIK_INSTANCE_NAME", instance)
        .env("MAIEUTIK_EMBEDDER_MODEL", sandbox.join("absent.onnx"))
        .env("MAIEUTIK_EMBEDDER_TOKENIZER", sandbox.join("absent.json"))
        .output()
        .expect("lancement du binaire maieutik")
}

#[test]
fn second_instance_is_refused_when_lock_is_held() {
    let sandbox = tempfile::tempdir().unwrap();
    let instance = instance_name("held");

    // Le test tient le verrou sous le nom exact que le binaire va ouvrir.
    let lock = SingleInstance::new(&instance).unwrap();
    assert!(lock.is_single(), "précondition : le test détient le verrou");

    let out = run(sandbox.path(), &instance, &["index"]);
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
    let out = run(sandbox.path(), &instance_name("free"), &["index"]);
    assert_ne!(
        code(&out),
        0,
        "sans modèle ni dossier, la sortie est non nulle"
    );
    assert!(
        !err(&out).contains("déjà en cours d'exécution"),
        "aucun message de second instance sans verrou détenu ; obtenu : {}",
        err(&out)
    );
}
