use base64::Engine;
use opencv::core::Vector;
use opencv::imgcodecs::{imencode, imread, ImreadModes};
use opencv::prelude::*;
use tauri_plugin_dialog::{DialogExt, FilePath};

use tauri::{AppHandle, Emitter, Manager};

use crate::errors::UploadError;
use crate::state::{AppState, StateMutex};

#[tauri::command]
pub fn upload_key_image(app: AppHandle) {
    println!("uploading key image");
    app.dialog()
        .file()
        .pick_file(move |file_path| match read_from_maybe_path(file_path) {
            Ok(mat) => match mat_to_base64_png(&mat) {
                Ok(base64_image) => {
                    let state = app.state::<StateMutex>();
                    let mut state = state.lock().unwrap();
                    *state = AppState::WithKeyImage { key: mat };
                    let _ = app.emit("key-upload", base64_image);
                    let _ = app.emit("key-status", "");
                }
                Err(e) => _ = app.emit("key-status", format!("{}", UploadError::from(e))),
            },
            Err(e) => _ = app.emit("key-status", format!("{e}")),
        });
}

fn mat_to_base64_png(mat: &Mat) -> Result<String, opencv::Error> {
    let mut buf: Vector<u8> = Vec::new().into();
    imencode(".png", mat, &mut buf, &Vec::new().into())?;
    let base64 = base64::prelude::BASE64_STANDARD.encode(&buf);
    Ok(format!("data:image/png;base64,{base64}"))
}

fn read_from_maybe_path(maybe_path: Option<FilePath>) -> Result<Mat, UploadError> {
    let file_path = maybe_path.ok_or(UploadError::Canceled)?;
    let path = file_path.into_path()?;
    let path_str = path.to_str().ok_or(UploadError::NonUtfPath)?;
    let mat = imread(path_str, ImreadModes::IMREAD_GRAYSCALE.into())
        .map_err(|_| UploadError::NotImage)?;
    Ok(mat)
}
