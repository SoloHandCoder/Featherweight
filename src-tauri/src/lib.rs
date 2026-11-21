mod actions;
mod catalog;
mod cleanup;
mod engine;
mod env;
mod journal;
mod startup;
pub mod model;
mod restore;
mod stats;
pub mod system;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
