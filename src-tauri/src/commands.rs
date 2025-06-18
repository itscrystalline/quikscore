use crate::image::upload_key_image_impl;
use crate::image::upload_sheet_images_impl;
use crate::AppState;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub fn upload_key_image(app: AppHandle) {
    println!("uploading key image");
    app.dialog().file().pick_file(move |file_path| {
        upload_key_image_impl(app, file_path);
    });
}

#[tauri::command]
pub fn clear_key_image(app: AppHandle) {
    AppState::clear_key(app);
}

#[tauri::command]
pub fn upload_sheet_images(app: AppHandle) {
    println!("uploading sheet images");
    app.dialog().file().pick_files(move |file_paths| {
        upload_sheet_images_impl(app, file_paths);
    });
}
#[tauri::command]
pub fn clear_sheet_images(app: AppHandle) {
    AppState::clear_answer_sheets(app);
}
