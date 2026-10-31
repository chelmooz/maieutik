use maieutik::config::store::settings_dir;
use std::path::PathBuf;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn config_dir_can_be_overridden_without_touching_home() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("MAIEUTIK_CONFIG_DIR");
    std::env::set_var("MAIEUTIK_CONFIG_DIR", dir.path());

    let resolved = settings_dir();

    match previous {
        Some(value) => std::env::set_var("MAIEUTIK_CONFIG_DIR", value),
        None => std::env::remove_var("MAIEUTIK_CONFIG_DIR"),
    }

    assert_eq!(resolved.unwrap(), PathBuf::from(dir.path()));
}
