use crate::errors::{SheetError, UploadError};
use crate::signal;
use base64::Engine;
use itertools::Itertools;
use opencv::core::{Mat, Range, Rect_, Size, Vector};
use opencv::imgproc::THRESH_BINARY;
use opencv::{highgui, imgproc, prelude::*};
use tauri_plugin_dialog::FilePath;

use tauri::{Emitter, Manager, Runtime};

use opencv::imgcodecs::{imencode, imread, ImreadModes};

use crate::state::{AppState, SignalKeys};

macro_rules! new_mat_copy {
    ($orig: ident) => {{
        let mut mat = Mat::default();
        mat.set_rows($orig.rows());
        mat.set_cols($orig.cols());
        mat
    }};
}

pub fn upload_key_image_impl<R: Runtime, A: Emitter<R> + Manager<R>>(
    app: &A,
    path_maybe: Option<FilePath>,
) {
    let Some(file_path) = path_maybe else {
        signal!(
            app,
            SignalKeys::KeyStatus,
            format!("{}", UploadError::Canceled)
        );
        return;
    };
    match handle_upload(file_path) {
        Ok((base64_image, mat)) => AppState::upload_key(app, base64_image, mat),
        Err(e) => signal!(app, SignalKeys::KeyStatus, format!("{e}")),
    }
}

pub fn upload_sheet_images_impl<R: Runtime, A: Emitter<R> + Manager<R>>(
    app: &A,
    paths: Option<Vec<FilePath>>,
) {
    let Some(paths) = paths else {
        signal!(
            app,
            SignalKeys::SheetStatus,
            format!("{}", UploadError::Canceled)
        );
        return;
    };

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
            let (vec_base64, vec_mat): (Vec<String>, Vec<Mat>) = vec.into_iter().multiunzip();
            AppState::upload_answer_sheets(app, vec_base64, vec_mat);
        }
        Err(e) => signal!(app, SignalKeys::SheetStatus, format!("{e}")),
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
    let (aligned_for_display, subject_id, student_id, answer_sheet) = fix_answer_sheet(resized)?;

    let subject_id_string = extract_digits_for_sub_stu(&subject_id, 2, false)?;
    let student_id_string = extract_digits_for_sub_stu(&student_id, 9, true)?;
    println!("subject_id: {subject_id_string}");
    println!("subject_id: {student_id_string}");
    //testing
    //#[cfg(not(test))]
    //let _ = show_img(&aligned_for_processing, "resized & aligned image");
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

fn fix_answer_sheet(mat: Mat) -> Result<(Mat, Mat, Mat, Mat), SheetError> {
    let cropped = crop_to_markers(mat)?;
    let preprocessed = preprocess_sheet(cropped.clone())?;

    let (subject_id, student_id, ans_sheet) = split_into_areas(preprocessed)?;

    Ok((cropped, subject_id, student_id, ans_sheet))
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

fn extract_digits_for_sub_stu(
    mat: &Mat,
    num_digits: i32,
    mut is_student_id: bool,
) -> Result<String, opencv::Error> {
    let the_height_from_above_to_bubble = 40;
    let overall_height = mat.rows() - the_height_from_above_to_bubble;
    let digit_height = overall_height / 10;
    let digit_width = mat.cols() / num_digits;
    let temp: bool = is_student_id;
    let rows;
    if is_student_id {
        rows = 8;
    } else {
        rows = 2;
    }
    let cols = 10;
    let mut v: Vec<Vec<i32>> = vec![vec![0; cols]; rows];

    let mut digits = String::new();

    for i in (0..num_digits as usize) {
        if is_student_id {
            is_student_id = false;
            continue;
        }
        let x = (i as i32) * digit_width;
        let roi = mat
            .roi(Rect_ {
                x,
                y: 0,
                width: digit_width,
                height: mat.rows(),
            })?
            .clone_pointee();

        let mut min_sum = u32::MAX;
        let mut selected_digit = 0;

        for j in (0..10 as usize) {
            let y = (j as i32) * digit_height + the_height_from_above_to_bubble;
            let digit_roi = roi.roi(Rect_ {
                x: 0,
                y,
                width: digit_width,
                height: digit_height,
            })?;

            let sum: u32 = digit_roi.data_bytes()?.iter().map(|&b| b as u32).sum();
            if temp {
                if i > 0 {
                    v[i - 1][j] = (sum as i32); //Skip first Index NaKub
                }
            } else {
                v[i][j] = (sum as i32);
            }

            if sum < min_sum {
                min_sum = sum;
                selected_digit = j;
            }
        }
        digits.push_str(&selected_digit.to_string());
    }
    if temp {
        println!("Stundet:");
    } else {
        println!("Subject");
    }
    for (i, row) in v.iter().enumerate() {
        println!("Row {}: {:?}", i, row);
    }
    Ok(digits)
}

#[cfg(test)]
mod unit_tests {
    use std::path::PathBuf;

    use super::*;
    use base64::prelude::*;
    use opencv::core;

    fn test_key_image() -> FilePath {
        FilePath::Path(PathBuf::from("tests/assets/sample_valid_image.jpg"))
    }

    fn test_images() -> Vec<FilePath> {
        vec![
            FilePath::Path(PathBuf::from("tests/assets/image_001.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/image_002.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/image_003.jpg")),
        ]
    }

    fn not_image() -> FilePath {
        FilePath::Path(PathBuf::from("tests/assets/sample_invalid_image.jpg"))
    }

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

    #[test]
    fn test_read_from_valid_path() {
        let path = test_key_image();
        let mat = read_from_path(path);
        assert!(mat.is_ok());
        let mat = mat.unwrap();
        assert!(!mat.empty());
    }

    #[test]
    fn test_read_from_invalid_image_file() {
        let path = not_image();
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
        let path = test_key_image();
        let result = handle_upload(path);
        assert!(result.is_ok());

        let (base64_string, mat /*, answer_sheet*/) = result.unwrap();
        assert!(base64_string.starts_with("data:image/png;base64,"));
        assert!(!mat.empty());
    }

    #[test]
    fn test_handle_upload_failure() {
        let path = not_image();
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

    #[test]
    fn test_preprocess_sheet_thresholding() {
        // Make a light gray image (above threshold)
        let mat = Mat::new_rows_cols_with_default(10, 10, core::CV_8UC1, core::Scalar::all(240.0))
            .unwrap();
        let thresh = preprocess_sheet(mat);
        assert!(thresh.is_ok());

        let result = thresh.unwrap();
        assert_eq!(result.at_2d::<u8>(0, 0).unwrap(), &255);
    }

    #[test]
    fn test_crop_to_markers_size() {
        let mat =
            Mat::new_rows_cols_with_default(900, 1200, core::CV_8UC1, core::Scalar::all(100.0))
                .unwrap();
        let cropped = crop_to_markers(mat);
        assert!(cropped.is_ok());
        let cropped = cropped.unwrap();
        assert_eq!(cropped.rows(), 765); // 795 - 30
        assert_eq!(cropped.cols(), 1095); // 1133 - 38
    }

    #[test]
    fn test_split_into_areas() {
        // Size must be at least (1090, 750) to cover all ROIs
        let mat =
            Mat::new_rows_cols_with_default(800, 1100, core::CV_8UC1, core::Scalar::all(200.0))
                .unwrap();
        let result = split_into_areas(mat);
        assert!(result.is_ok());

        let (subject, student_id, answers) = result.unwrap();
        assert_eq!(subject.rows(), 212);
        assert_eq!(subject.cols(), 48);
        assert_eq!(student_id.rows(), 211);
        assert_eq!(student_id.cols(), 141);
        assert_eq!(answers.rows(), 735);
        assert_eq!(answers.cols(), 884);
    }

    #[test]
    fn check_extracted_ids_from_real_image() {
        let path = test_key_image();
        let mat = read_from_path(path).expect("Failed to read image");
        let resized = resize_img(mat).expect("Resize failed");
        let (_cropped, subject_id_mat, student_id_mat, _answers) =
            fix_answer_sheet(resized).expect("Fixing sheet failed");

        let subject_id = extract_digits_for_sub_stu(&subject_id_mat, 2, false)
            .expect("Extracting subject ID failed");
        let student_id = extract_digits_for_sub_stu(&student_id_mat, 9, true)
            .expect("Extracting student ID failed");

        assert_eq!(subject_id, "10", "Subject ID does not match expected value");
        assert_eq!(
            student_id, "65010001",
            "Student ID does not match expected value"
        );
    }
}
