use crate::state::AppState;
use std::sync::Mutex;
use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod commands;
mod errors;
mod image;
mod scoring;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::upload_key_image,
            commands::clear_key_image,
            commands::upload_sheet_images,
            commands::clear_sheet_images
        ])
        .setup(|app| {
            app.manage(Mutex::new(AppState::Init));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
