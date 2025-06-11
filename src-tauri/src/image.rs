use base64::Engine;
use opencv::core::Vector;
use opencv::imgcodecs::{imencode, imread, ImreadModes};
use opencv::prelude::*;
use tauri_plugin_dialog::{DialogExt, FilePath};

use tauri::{AppHandle, Emitter, Manager};

use crate::errors::UploadError;
use crate::signal;
use crate::state::{AppState, SignalKeys, StateMutex};

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
        match handle_upload(file_path) {
            Ok((base64_image, mat)) => {
                let mutex = app.state::<StateMutex>();
                let mut state = mutex.lock().unwrap();
                match *state {
                    AppState::Init | AppState::WithKeyImage { .. } => {
                        *state = AppState::WithKeyImage { key: mat };
                        signal!(app, SignalKeys::KeyImage, base64_image);
                        signal!(app, SignalKeys::KeyStatus, "");
                    }
                    _ => (),
                }
            }
            Err(e) => signal!(app, SignalKeys::KeyStatus, format!("{e}")),
        }
    });
}

#[tauri::command]
pub fn clear_key_image(app: AppHandle) {
    let mutex = app.state::<StateMutex>();
    let mut state = mutex.lock().unwrap();
    if let AppState::WithKeyImage { .. } = *state {
        *state = AppState::Init;
        signal!(app, SignalKeys::KeyImage, "");
        signal!(app, SignalKeys::KeyStatus, "");
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
                match file_paths.ok_or(UploadError::Canceled) {
                    Ok(file_paths) => {
                        let base64_list: Result<Vec<(String, Mat)>, UploadError> = file_paths
                            .into_iter()
                            .enumerate()
                            .map(|(idx, file_path)| {
                                signal!(
                                    app,
                                    SignalKeys::SheetStatus,
                                    format!("Processing image #{}", idx + 1)
                                );
                                handle_upload(file_path)
                            })
                            .collect();
                        match base64_list {
                            Ok(vec) => {
                                let (vec_base64, vec_mat): (Vec<String>, Vec<Mat>) =
                                    vec.into_iter().unzip();
                                *state = AppState::WithKeyAndSheets {
                                    key: key.clone(),
                                    _sheets: vec_mat,
                                };
                                signal!(app, SignalKeys::SheetImages, vec_base64);
                                signal!(app, SignalKeys::SheetStatus, "");
                            }
                            Err(e) => signal!(app, SignalKeys::SheetStatus, format!("{e}")),
                        }
                    }
                    Err(e) => signal!(app, SignalKeys::SheetStatus, format!("{e}")),
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
        signal!(app, SignalKeys::SheetImages, Vec::<String>::new());
        signal!(app, SignalKeys::SheetStatus, "");
    }
}

fn handle_upload(path: FilePath) -> Result<(String, Mat), UploadError> {
    let mat = read_from_path(path)?;
    let base64 = mat_to_base64_png(&mat).map_err(UploadError::from)?;
    Ok((base64, mat))
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

#[cfg(test)]
mod unit_tests {
    use super::*;
    use base64::prelude::*;
    use opencv::{core, imgcodecs, prelude::*};

    #[test]
    fn test_basic_functionality() {
        // Create a 2x2 black image (3 channels, 8-bit)
        let mat =
            Mat::new_rows_cols_with_default(2, 2, core::CV_8UC3, core::Scalar::all(0.0)).unwrap();

        let result = mat_to_base64_png(&mat);
        assert!(result.is_ok());

        let data_url = result.unwrap();
        assert!(data_url.starts_with("data:image/png;base64,"));

        // Check PNG signature after decoding base64
        let b64_data = data_url.strip_prefix("data:image/png;base64,").unwrap();
        let decoded_bytes = BASE64_STANDARD.decode(b64_data).unwrap();
        // PNG signature bytes
        let png_signature = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(&decoded_bytes[0..8], &png_signature);
    }

    #[test]
    fn test_empty_mat_should_fail() {
        // Create an empty Mat
        let mat = Mat::default();
        let result = mat_to_base64_png(&mat);
        assert!(result.is_err());
    }
}
