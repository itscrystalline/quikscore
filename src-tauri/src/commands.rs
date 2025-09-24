#[cfg(not(feature = "compile-tesseract"))]
use crate::errors::OcrError;
use crate::{
    download::{self, ModelDownload},
    err_log,
    errors::ModelDownloadError,
    image::{upload_key_image_impl, upload_sheet_images_impl},
    ocr::OcrEngine,
    scoring::upload_weights_impl,
    state::{AnswerUpload, CsvExport, KeyUpload, LoginRequest, LoginResponse},
    storage, AppState,
};
use log::{debug, info};
use reqwest::Client;

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
pub fn set_ocr(app: AppHandle, ocr: bool) -> Result<(), String> {
    let has_tess = OcrEngine::check_tesseract().map_err(|e| {
        err_log!(&e);
        format!("{e}")
    })?;
    if has_tess {
        debug!("Set ocr = {ocr}");
        AppState::set_ocr(&app, ocr);
        Ok(())
    } else {
        #[cfg(not(feature = "compile-tesseract"))]
        {
            Err(format!("{}", OcrError::NoTesseract))
        }
        #[cfg(feature = "compile-tesseract")]
        {
            // with `compile-tesseract`, `OcrEngine::check_tesseract()` always returns true
            unreachable!()
        }
    }
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
pub fn enter_database_information(app: AppHandle, uri: String, name: String) {
    info!("Enter Database Information");
    AppState::set_mongodb(&app, uri, name);
}

#[tauri::command]
pub async fn login(app: AppHandle, username: String, password: String) -> Result<(), String> {
    let client = Client::new();
    let res = client
        .post("http://localhost:5000/login")
        .json(&LoginRequest { username, password })
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let body: LoginResponse = res.json().await.map_err(|e| e.to_string())?;
    if body.success {
        info!("Authentication Passed!");
        WebviewWindowBuilder::from_config(
            &app,
            app.config()
                .app
                .windows
                .first()
                .ok_or("the main window is not present in the config".to_string())?,
        )
        .map_err(|e| {
            err_log!(&e);
            "cannot create main window".to_string()
        })?
        .build()
        .map_err(|e| {
            err_log!(&e);
            "cannot create main window".to_string()
        })?;
        app.webview_windows()
            .get("auth")
            .ok_or("missing auth window".to_string())?
            .close()
            .map_err(|e| {
                err_log!(&e);
                "cannot close auth window".to_string()
            })
    } else {
        Err("Invalid username or password".to_string())
    }
}

#[tauri::command]
pub fn image_of(app: AppHandle, id: String) -> Option<Vec<u8>> {
    AppState::get_base64_for_id(&app, id)
}
