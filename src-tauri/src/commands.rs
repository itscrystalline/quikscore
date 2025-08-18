use crate::{
    download::{self, ModelDownload},
    errors::ModelDownloadError,
    image::{upload_key_image_impl, upload_sheet_images_impl},
    scoring::upload_weights_impl,
    state::{AnswerUpload, CsvExport, KeyUpload},
    storage, AppState,
};
use log::{debug, info};

use tauri::{ipc::Channel, WebviewWindowBuilder};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

#[tauri::command(async)]
pub async fn auth_pass(app: AppHandle) {
    info!("Authentication Passed!");
    WebviewWindowBuilder::from_config(
        &app,
        app.config()
            .app
            .windows
            .first()
            .expect("the main window is not present in the config"),
    )
    .expect("cannot create main window")
    .build()
    .expect("cannot create main window");
    app.webview_windows()
        .get("auth")
        .expect("missing auth window")
        .close()
        .expect("cannot close auth window");
}

#[tauri::command]
pub fn upload_key_image(app: AppHandle, channel: Channel<KeyUpload>) {
    info!("Uploading key image");
    app.dialog().file().pick_file(move |file_path| {
        upload_key_image_impl(&app, file_path, channel);
    });
}

#[tauri::command]
pub fn upload_weights(app: AppHandle, channel: Channel<KeyUpload>) {
    info!("Uploading weights");
    app.dialog().file().pick_file(move |file_path| {
        upload_weights_impl(&app, file_path, channel);
    });
}

#[tauri::command]
pub fn clear_key_image(app: AppHandle, channel: Channel<KeyUpload>) {
    info!("Clearing key image");
    AppState::clear_key(&app, &channel);
}

#[tauri::command]
pub fn clear_weights(app: AppHandle, channel: Channel<KeyUpload>) {
    info!("Clearing weights");
    AppState::clear_weights(&app, &channel);
}

#[tauri::command]
pub fn upload_sheet_images(app: AppHandle, channel: Channel<AnswerUpload>) {
    info!("Uploading sheet images");
    app.dialog().file().pick_files(move |file_paths| {
        upload_sheet_images_impl(&app, file_paths, channel);
    });
}
#[tauri::command]
pub fn cancel_upload_sheets(app: AppHandle, channel: Channel<AnswerUpload>) {
    info!("Sheet upload cancelled");
    AppState::cancel_scoring(&app, &channel);
}
#[tauri::command]
pub fn clear_sheet_images(app: AppHandle, channel: Channel<AnswerUpload>) {
    info!("Clearing sheets");
    AppState::clear_answer_sheets(&app, &channel);
}

#[tauri::command]
pub fn set_ocr(app: AppHandle, ocr: bool) {
    debug!("Set ocr = {ocr}");
    AppState::set_ocr(&app, ocr);
}

#[tauri::command(async)]
pub async fn ensure_models(
    app: AppHandle,
    channel: Channel<ModelDownload>,
) -> Result<(), ModelDownloadError> {
    info!("Ensuring OCR models");
    download::get_or_download_models(app, channel).await
}

#[tauri::command]
pub fn export_csv(app: AppHandle, channel: Channel<CsvExport>) {
    info!("Exporting results");
    app.dialog()
        .file()
        .add_filter("Comma Seperated Value files (*.csv)", &["csv"])
        .save_file(move |file_path| {
            storage::export_to_csv_wrapper(&app, file_path, channel);
        });
}

#[tauri::command]
pub fn enter_database_infomation(app: AppHandle, mongodb_uri: String, mongodb_name: String) {
    info!("Enter Database Information");
    AppState::set_mongodb(&app, mongodb_uri, mongodb_name);
}
