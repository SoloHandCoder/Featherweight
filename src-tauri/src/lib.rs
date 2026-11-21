mod actions;
mod catalog;
mod cleanup;
mod commands;
mod datadir;
mod engine;
mod env;
mod journal;
pub mod model;
mod restore;
mod startup;
mod stats;
pub mod system;

use std::sync::{Arc, Mutex};

use catalog::Catalog;
use commands::AppState;
use engine::Engine;
use journal::Journal;
use restore::RestoreGuard;
use system::windows::WindowsSystem;

/// Secures the data folder and loads the journal. On failure the app still
/// opens, with an unwritable journal and the reason shown by `get_overview`.
fn load_journal() -> (Journal, Option<String>) {
    let loaded = datadir::secure_data_dir().and_then(|dir| {
        Journal::load(dir.join(datadir::JOURNAL_FILE)).map_err(|e| e.to_string())
    });
    match loaded {
        Ok(j) => (j, None),
        Err(e) => (Journal::unavailable(), Some(e)),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (journal, journal_error) = load_journal();
    let engine = Engine::new(WindowsSystem::new(), Catalog::bundled(), journal, env::detect_env());
    let state = AppState {
        engine: Arc::new(Mutex::new(engine)),
        restore: Arc::new(Mutex::new(RestoreGuard::default())),
        journal_error,
    };
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_overview,
            commands::list_tweaks,
            commands::preview_preset,
            commands::apply_tweaks,
            commands::undo_tweaks,
            commands::undo_all,
            commands::ensure_restore_point,
            commands::enable_system_restore,
            commands::list_startup,
            commands::set_startup_enabled,
            commands::scan_cleanup,
            commands::run_cleanup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
