use crate::state;
use crate::state::CsvExport;
use crate::storage;
use std::path::PathBuf;

use crate::{
    download::{self, ModelDownload},
    errors::ModelDownloadError,
    image::{upload_key_image_impl, upload_sheet_images_impl},
    scoring::upload_weights_impl,
    state::{self, AnswerUpload, CsvExport, KeyUpload},
    storage, AppState,
};

use tauri::ipc::Channel;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub fn upload_key_image(app: AppHandle, channel: Channel<KeyUpload>) {
    println!("uploading key image");
    app.dialog().file().pick_file(move |file_path| {
        upload_key_image_impl(&app, file_path, channel);
    });
}

#[tauri::command]
pub fn upload_weights(app: AppHandle, channel: Channel<KeyUpload>) {
    println!("uploading weights");
    app.dialog().file().pick_file(move |file_path| {
        upload_weights_impl(&app, file_path, channel);
    });
}

#[tauri::command]
pub fn clear_key_image(app: AppHandle, channel: Channel<KeyUpload>) {
    AppState::clear_key(&app, &channel);
}

#[tauri::command]
pub fn clear_weights(app: AppHandle, channel: Channel<KeyUpload>) {
    AppState::clear_weights(&app, &channel);
}

#[tauri::command]
pub fn upload_sheet_images(app: AppHandle, channel: Channel<AnswerUpload>) {
    println!("uploading sheet images");
    app.dialog().file().pick_files(move |file_paths| {
        upload_sheet_images_impl(&app, file_paths, channel);
    });
}
#[tauri::command]
pub fn cancel_upload_sheets(app: AppHandle, channel: Channel<AnswerUpload>) {
    AppState::cancel_scoring(&app, &channel);
}
#[tauri::command]
pub fn clear_sheet_images(app: AppHandle, channel: Channel<AnswerUpload>) {
    AppState::clear_answer_sheets(&app, &channel);
}

#[tauri::command]
pub fn set_ocr(app: AppHandle, ocr: bool) {
    AppState::set_ocr(&app, ocr);
}

#[tauri::command(async)]
pub async fn ensure_models(
    app: AppHandle,
    channel: Channel<ModelDownload>,
) -> Result<(), ModelDownloadError> {
    download::get_or_download_models(app, channel).await
}

#[tauri::command]
pub fn export_csv(app: AppHandle, channel: Channel<CsvExport>) {
    println!("exporting results");
    app.dialog()
        .file()
        .add_filter("Comma Seperated Value files (*.csv)", &["csv"])
        .save_file(move |file_path| {
            storage::export_to_csv_wrapper(&app, file_path, channel);
        });
}
