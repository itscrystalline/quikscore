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
        upload_key_image_impl(app, file_path);
    });
}

fn upload_key_image_impl(app: AppHandle, path: FilePath) {
    match handle_upload(path) {
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

fn upload_sheet_images_impl(app: AppHandle, paths: Vec<FilePath>) {
    let mutex = app.state::<StateMutex>();
    let mut state = mutex.lock().unwrap();
    match *state {
        AppState::WithKeyImage { ref key } | AppState::WithKeyAndSheets { ref key, .. } => {
            let base64_list: Result<Vec<(String, Mat)>, UploadError> = paths
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
                    let (vec_base64, vec_mat): (Vec<String>, Vec<Mat>) = vec.into_iter().unzip();
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
        _ => (),
    }
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

fn read_from_path(path: FilePath) -> Result<Mat, UploadError> {
    let path = path.into_path()?;
    let path_str = path.to_str().ok_or(UploadError::NonUtfPath)?;
    imread(path_str, ImreadModes::IMREAD_GRAYSCALE.into())
        .map_err(|_| UploadError::NotImage)
        .and_then(|mat| {
            if mat.empty() {
                Err(UploadError::NotImage)
            } else {
                Ok(mat)
            }
        })
}

#[cfg(test)]
mod unit_tests {
    use std::path::PathBuf;

    use super::*;
    use base64::prelude::*;
    use opencv::core;

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

    fn test_image_path() -> PathBuf {
        PathBuf::from("tests/assets/sample_valid_image.jpg")
    }

    fn non_image_path() -> PathBuf {
        PathBuf::from("tests/assets/sample_invalid_image.jpg")
    }

    #[test]
    fn test_read_from_valid_path() {
        let path = FilePath::Path(test_image_path());
        let mat = read_from_path(path);
        assert!(mat.is_ok());
        let mat = mat.unwrap();
        assert!(!mat.empty());
    }

    #[test]
    fn test_read_from_invalid_image_file() {
        let path = FilePath::Path(non_image_path());
        let result = read_from_path(path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), UploadError::NotImage));
    }

    #[test]
    fn test_read_from_non_utf8_path() {
        // This simulates a non-UTF-8 path by using invalid UTF-8 bytes
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let non_utf8 = OsStr::from_bytes(b"\xff\xfe").to_os_string();
        let path = FilePath::Path(PathBuf::from(non_utf8));

        let result = read_from_path(path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), UploadError::NonUtfPath));
    }

    #[test]
    fn test_handle_upload_success() {
        let path = FilePath::Path(test_image_path());
        let result = handle_upload(path);
        assert!(result.is_ok());

        let (base64_string, mat) = result.unwrap();
        assert!(base64_string.starts_with("data:image/png;base64,"));
        assert!(!mat.empty());
    }

    #[test]
    fn test_handle_upload_failure() {
        let path = FilePath::Path(non_image_path());
        let result = handle_upload(path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), UploadError::NotImage));
    }
}
