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
    app.dialog().file().pick_file(move |file_path| {
        if let Some(file_path) = file_path {
            match read_from_path(file_path) {
                Ok(mat) => match mat_to_base64_png(&mat) {
                    Ok(base64_image) => {
                        let mutex = app.state::<StateMutex>();
                        let mut state = mutex.lock().unwrap();
                        match *state {
                            AppState::Init | AppState::WithKeyImage { .. } => {
                                *state = AppState::WithKeyImage { key: mat };
                                let _ = app.emit("key-image", base64_image);
                                let _ = app.emit("key-status", "");
                            }
                            _ => (),
                        }
                    }
                    Err(e) => _ = app.emit("key-status", format!("{}", UploadError::from(e))),
                },
                Err(e) => _ = app.emit("key-status", format!("{e}")),
            }
        }
    });
}

#[tauri::command]
pub fn clear_key_image(app: AppHandle) {
    let mutex = app.state::<StateMutex>();
    let mut state = mutex.lock().unwrap();
    if let AppState::WithKeyImage { .. } = *state {
        *state = AppState::Init;
        _ = app.emit("key-image", "");
        _ = app.emit("key-status", "");
    }
}

#[tauri::command]
pub fn upload_sheet_images(app: AppHandle) {
    println!("uploading sheet images");
    app.dialog().file().pick_files(move |file_paths| {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().unwrap();
        match *state {
            AppState::WithKeyImage { ref key } | AppState::WithKeyAndSheets { ref key, .. } => {
                if let Some(file_paths) = file_paths {
                    let base64_list: Result<Vec<(String, Mat)>, UploadError> = file_paths
                        .into_iter()
                        .map(|file_path| {
                            let mat = read_from_path(file_path)?;
                            let base64 = mat_to_base64_png(&mat).map_err(UploadError::from)?;
                            Ok((base64, mat))
                        })
                        .collect();
                    match base64_list {
                        Ok(vec) => {
                            let (vec_base64, vec_mat): (Vec<String>, Vec<Mat>) =
                                vec.into_iter().unzip();
                            *state = AppState::WithKeyAndSheets {
                                key: key.clone(),
                                sheets: vec_mat,
                            };
                            _ = app.emit("sheet-images", vec_base64);
                            _ = app.emit("sheet-status", "");
                        }
                        Err(e) => _ = app.emit("sheet-status", format!("{e}")),
                    }
                }
            }
            _ => (),
        }
    });
}

#[tauri::command]
pub fn clear_sheet_images(app: AppHandle) {
    let mutex = app.state::<StateMutex>();
    let mut state = mutex.lock().unwrap();
    if let AppState::WithKeyAndSheets { ref key, .. } = *state {
        *state = AppState::WithKeyImage { key: key.clone() };
        _ = app.emit("sheet-images", Vec::<String>::new());
        _ = app.emit("sheet-status", "");
    }
}

fn mat_to_base64_png(mat: &Mat) -> Result<String, opencv::Error> {
    let mut buf: Vector<u8> = Vec::new().into();
    imencode(".png", mat, &mut buf, &Vec::new().into())?;
    let base64 = base64::prelude::BASE64_STANDARD.encode(&buf);
    Ok(format!("data:image/png;base64,{base64}"))
}

fn read_from_path(file_path: FilePath) -> Result<Mat, UploadError> {
    let path = file_path.into_path()?;
    let path_str = path.to_str().ok_or(UploadError::NonUtfPath)?;
    let mat = imread(path_str, ImreadModes::IMREAD_GRAYSCALE.into())
        .map_err(|_| UploadError::NotImage)?;
    Ok(mat)
}
