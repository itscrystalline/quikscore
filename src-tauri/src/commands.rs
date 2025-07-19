use crate::state;
use std::path::PathBuf;

use crate::image::upload_key_image_impl;
use crate::image::upload_sheet_images_impl;
use crate::state::AnswerUpload;
use crate::state::KeyUpload;
use crate::AppState;
use tauri::ipc::Channel;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub fn upload_key_image(app: AppHandle, channel: Channel<KeyUpload>, model_dir: PathBuf) {
    state::init_model_dir(model_dir);
    println!("uploading key image");
    app.dialog().file().pick_file(move |file_path| {
        upload_key_image_impl(&app, file_path, channel);
    });
}

#[tauri::command]
pub fn clear_key_image(app: AppHandle, channel: Channel<KeyUpload>) {
    AppState::clear_key(&app, channel);
}

#[tauri::command]
pub fn upload_sheet_images(app: AppHandle, channel: Channel<AnswerUpload>, model_dir: PathBuf) {
    state::init_model_dir(model_dir);
    println!("uploading sheet images");
    app.dialog().file().pick_files(move |file_paths| {
        upload_sheet_images_impl(&app, file_paths, channel);
    });
}
#[tauri::command]
pub fn clear_sheet_images(app: AppHandle, channel: Channel<AnswerUpload>) {
    AppState::clear_answer_sheets(&app, channel);
}
