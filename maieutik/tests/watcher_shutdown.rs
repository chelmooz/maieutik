//! Arrêt propre du watcher.
//!
//! Trois contrats : le processeur de débounce rend la main quand son canal
//! d'entrée se ferme (il ne doit pas rester à tourner sur son intervalle), les
//! événements encore en attente sont vidés au lieu d'être perdus, et
//! `run_watcher_with_shutdown` retourne sur signal d'arrêt.

use maieutik::config::store::ConfigStore;
use maieutik::core::watch::{DebouncedProcessor, WatchEvent};
use maieutik::{run_watcher_with_shutdown, AppState};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// État sans dossier surveillé ni index : le watcher démarre sans toucher au
/// disque de l'utilisateur.
///
/// `MAIEUTIK_CONFIG_DIR` isole la lecture de la configuration : sans cela,
/// `ConfigStore::new` lit `~/.config/maieutik/config.json` et le test échoue
/// dès que ce fichier pointe vers un dossier inaccessible. Le répertoire
/// temporaire est conservé (`keep`) pour rester valide pendant tout le test.
async fn app_state() -> Arc<AppState> {
    let config_dir = tempfile::tempdir().unwrap().keep();
    std::env::set_var("MAIEUTIK_CONFIG_DIR", &config_dir);

    Arc::new(AppState {
        config: Arc::new(ConfigStore::new(None).await.unwrap()),
        vector_index: None,
        embedder: None,
    })
}

#[tokio::test]
async fn processor_stops_when_input_closes() {
    let (tx, rx) = mpsc::channel(8);
    let (process_tx, _process_rx) = mpsc::channel(8);
    let handle = tokio::spawn(DebouncedProcessor::new(rx, process_tx).run());
    drop(tx);

    tokio::time::timeout(Duration::from_secs(1), handle)
        .await
        .expect("le processeur doit s'arrêter quand le canal d'entrée se ferme")
        .expect("la tâche ne doit pas paniquer");
}

#[tokio::test]
async fn pending_events_are_flushed_on_close() {
    let (tx, rx) = mpsc::channel(8);
    let (process_tx, mut process_rx) = mpsc::channel(8);
    let handle = tokio::spawn(DebouncedProcessor::new(rx, process_tx).run());

    // Arrivé juste avant la fermeture : bien en deçà de la fenêtre de
    // débounce de 2 s, il ne serait jamais émis sans vidage explicite.
    tx.send(WatchEvent::Created("/notes/a.md".into()))
        .await
        .unwrap();
    drop(tx);

    let event = tokio::time::timeout(Duration::from_secs(1), process_rx.recv())
        .await
        .expect("un événement en attente ne doit pas être perdu à l'arrêt");
    assert!(
        matches!(event, Some(WatchEvent::Created(_))),
        "événement inattendu : {event:?}"
    );
    let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
}

#[tokio::test]
async fn watcher_returns_on_shutdown_signal() {
    let (tx, rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(run_watcher_with_shutdown(app_state().await, async move {
        let _ = rx.await;
    }));

    tx.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("run_watcher doit rendre la main sur signal d'arrêt")
        .expect("la tâche ne doit pas paniquer")
        .expect("l'arrêt doit être un succès");
}
