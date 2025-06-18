use crate::errors::{SheetError, UploadError};
use crate::signal;
use base64::Engine;
use itertools::Itertools;
use opencv::core::{Mat, Range, Rect_, Size, Vector};
use opencv::imgproc::THRESH_BINARY;
use opencv::{highgui, imgproc, prelude::*};
use tauri_plugin_dialog::FilePath;

use tauri::{AppHandle, Emitter, Manager};

use opencv::imgcodecs::{imencode, imread, ImreadModes};

use crate::state::{AnswerSheet, AppState, SignalKeys, StateMutex};

macro_rules! new_mat_copy {
    ($orig: ident) => {{
        let mut mat = Mat::default();
        mat.set_rows($orig.rows());
        mat.set_cols($orig.cols());
        mat
    }};
}

pub fn upload_key_image_impl(app: AppHandle, path: FilePath) {
    match handle_upload(path) {
        Ok((base64_image, mat)) => {
            let mutex = app.state::<StateMutex>();
            let mut state = mutex.lock().unwrap();
            match *state {
                AppState::Init | AppState::WithKey { .. } => {
                    *state = AppState::WithKey {
                        key_image: mat,
                        // key: answer.into(),
                    };
                    signal!(app, SignalKeys::KeyImage, base64_image);
                    signal!(app, SignalKeys::KeyStatus, "");
                }
                _ => (),
            }
        }
        Err(e) => signal!(app, SignalKeys::KeyStatus, format!("{e}")),
    }
}

pub fn upload_sheet_images_impl(app: AppHandle, paths: Vec<FilePath>) {
    let mutex = app.state::<StateMutex>();
    let mut state = mutex.lock().unwrap();
    match *state {
        AppState::WithKey {
            ref key_image,
            // ref key,
        }
        | AppState::WithKeyAndSheets {
            ref key_image,
            // ref key,
            ..
        } => {
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
                    let (vec_base64, vec_mat): (Vec<String>, Vec<Mat>) =
                        vec.into_iter().multiunzip();
                    *state = AppState::WithKeyAndSheets {
                        key_image: key_image.clone(),
                        // key: key.clone(),
                        _sheet_images: vec_mat,
                        // _answer_sheets: vec_answers,
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

fn resize_img(src: Mat) -> opencv::Result<Mat> {
    let mut dst = new_mat_copy!(src);
    let new_size = Size::new(src.cols() / 3, src.rows() / 3);

    imgproc::resize(&src, &mut dst, new_size, 0.0, 0.0, imgproc::INTER_LINEAR)?;
    Ok(dst)
}

fn show_img(mat: &Mat, window_name: &str) -> opencv::Result<()> {
    println!("showing window {window_name}");
    highgui::named_window(window_name, 0)?;
    highgui::imshow(window_name, mat)?;
    highgui::wait_key(10000)?;
    Ok(())
}

fn handle_upload(path: FilePath) -> Result<(String, Mat), UploadError> {
    let mat = read_from_path(path)?;
    let resized = resize_img(mat).map_err(UploadError::from)?;
    let (aligned_for_display, aligned_for_processing) = fix_answer_sheet(resized)?;
    //testing
    #[cfg(not(test))]
    let _ = show_img(&aligned_for_processing, "resized & aligned image");
    let base64 = mat_to_base64_png(&aligned_for_display).map_err(UploadError::from)?;
    Ok((base64, aligned_for_display))
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

fn preprocess_sheet(mat: Mat) -> Result<Mat, SheetError> {
    // blur
    // let mut mat_blur = new_mat_copy!(mat);
    // imgproc::gaussian_blur_def(&mat, &mut mat_blur, (5, 5).into(), 0.0)?;
    // thresholding
    let mut mat_thresh = new_mat_copy!(mat);
    imgproc::threshold(&mat, &mut mat_thresh, 230.0, 255.0, THRESH_BINARY)?;
    Ok(mat_thresh)
}

fn crop_to_markers(mat: Mat) -> Result<Mat, SheetError> {
    Ok(mat
        .col_range(&Range::new(38, 1133)?)?
        .row_range(&Range::new(30, 795)?)?
        .clone_pointee())
}

fn fix_answer_sheet(mat: Mat) -> Result<(Mat, Mat), SheetError> {
    let cropped = crop_to_markers(mat)?;
    let preprocessed = preprocess_sheet(cropped.clone())?;

    Ok((cropped, preprocessed))
}

fn split_into_areas(sheet: Mat) -> Result<(Mat, Mat, Mat), SheetError> {
    let subject_area = sheet
        .roi(Rect_ {
            x: 2,
            y: 189,
            width: 48,
            height: 212,
        })?
        .clone_pointee();
    let student_id_area = sheet
        .roi(Rect_ {
            x: 57,
            y: 188,
            width: 141,
            height: 211,
        })?
        .clone_pointee();
    let answers_area = sheet
        .roi(Rect_ {
            x: 206,
            y: 14,
            width: 884,
            height: 735,
        })?
        .clone_pointee();

    Ok((subject_area, student_id_area, answers_area))
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
    #[cfg(unix)]
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

        let (base64_string, mat /*, answer_sheet*/) = result.unwrap();
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
    #[test]
    fn test_resize_img() {
        let width = 300;
        let height = 300;
        let mat =
            Mat::new_rows_cols_with_default(height, width, core::CV_8UC1, core::Scalar::all(128.0))
                .unwrap();

        let resized = resize_img(mat);
        assert!(resized.is_ok());

        let resized = resized.unwrap();
        assert!(!resized.empty());

        assert_eq!(resized.cols(), width / 3);
        assert_eq!(resized.rows(), height / 3);
    }
}
