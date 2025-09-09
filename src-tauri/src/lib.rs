#![feature(stmt_expr_attributes)]

use crate::state::AppState;
use std::sync::Mutex;
use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod commands;
mod download;
mod errors;
mod image;
mod ocr;
mod scoring;
mod state;
mod storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::auth_pass,
            commands::upload_key_image,
            commands::upload_weights,
            commands::clear_key_image,
            commands::clear_weights,
            commands::upload_sheet_images,
            commands::cancel_upload_sheets,
            commands::clear_sheet_images,
            commands::set_ocr,
            commands::ensure_models,
            commands::export_csv,
            commands::enter_database_information,
            commands::login,
            commands::image_of,
        ])
        .setup(|app| {
            app.manage(Mutex::new(AppState::default()));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
