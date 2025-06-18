use crate::errors::UploadError;
use crate::image::upload_key_image_impl;
use crate::image::upload_sheet_images_impl;
use crate::state::SignalKeys;
use crate::state::StateMutex;
use crate::AppState;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::signal;

#[tauri::command]
pub fn upload_key_image(app: AppHandle) {
    println!("uploading key image");
    app.dialog().file().pick_file(move |file_path| {
        let Some(file_path) = file_path else {
            signal!(
                app,
                SignalKeys::KeyStatus,
                format!("{}", UploadError::Canceled)
            );
            return;
        };
        upload_key_image_impl(app, file_path);
    });
}

#[tauri::command]
pub fn clear_key_image(app: AppHandle) {
    let mutex = app.state::<StateMutex>();
    let mut state = mutex.lock().unwrap();
    if let AppState::WithKey { .. } = *state {
        *state = AppState::Init;
        signal!(app, SignalKeys::KeyImage, "");
        signal!(app, SignalKeys::KeyStatus, "");
    }
}

#[tauri::command]
pub fn upload_sheet_images(app: AppHandle) {
    println!("uploading sheet images");
    app.dialog().file().pick_files(move |file_paths| {
        let Some(file_paths) = file_paths else {
            signal!(
                app,
                SignalKeys::SheetStatus,
                format!("{}", UploadError::Canceled)
            );
            return;
        };
        upload_sheet_images_impl(app, file_paths);
    });
}
#[tauri::command]
pub fn clear_sheet_images(app: AppHandle) {
    let mutex = app.state::<StateMutex>();
    let mut state = mutex.lock().unwrap();
    if let AppState::WithKeyAndSheets {
        /*key,*/ ref key_image,
        ..
    } = *state
    {
        *state = AppState::WithKey {
            key_image: key_image.clone(),
            // key,
        };
        signal!(app, SignalKeys::SheetImages, Vec::<String>::new());
        signal!(app, SignalKeys::SheetStatus, "");
    }
}
