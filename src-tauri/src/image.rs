use std::array;
use tauri::ipc::Channel;

use crate::errors::{SheetError, UploadError};
use crate::scoring::{AnswerSheetResult, CheckedAnswer};
use crate::{signal, state};
use base64::Engine;
use itertools::Itertools;
use opencv::core::{Mat, Rect_, Size, Vector};
use opencv::imgproc::{COLOR_GRAY2RGBA, FILLED, LINE_8, THRESH_BINARY};
use opencv::{imgcodecs, imgproc, prelude::*};
use rayon::prelude::*;
use std::fs;
use std::path::Path;
use tauri_plugin_dialog::FilePath;
use tesseract_rs::TesseractAPI;

use tauri::{Emitter, Manager, Runtime};

use opencv::imgcodecs::{imencode, imread, ImreadModes};

use crate::state::{Answer, AnswerSheet, AnswerUpload, AppState, KeyUpload, QuestionGroup};

macro_rules! new_mat_copy {
    ($orig: ident) => {{
        let mut mat = Mat::default();
        mat.set_rows($orig.rows());
        mat.set_cols($orig.cols());
        mat
    }};
}

const ANSWER_WIDTH: i32 = 215;
const ANSWER_WIDTH_GAP: i32 = 9;
const ANSWER_HEIGHT: i32 = 73;
const ANSWER_HEIGHT_GAP: i32 = 10;
const MARKER_TRANSPARENCY: f64 = 0.3;

pub fn upload_key_image_impl<R: Runtime, A: Emitter<R> + Manager<R>>(
    app: &A,
    path_maybe: Option<FilePath>,
    channel: Channel<KeyUpload>,
) {
    let Some(file_path) = path_maybe else {
        signal!(channel, KeyUpload::Cancelled);
        return;
    };
    match handle_upload(file_path, &state::init_thread_tesseract()) {
        Ok((base64_image, mat, key)) => {
            let subject = key.subject_code.clone();
            AppState::upload_key(app, channel, base64_image, mat, subject, key.into())
        }
        Err(e) => signal!(
            channel,
            KeyUpload::Error {
                error: format!("{e}")
            }
        ),
    }
}

pub fn upload_sheet_images_impl<R: Runtime, A: Emitter<R> + Manager<R>>(
    app: &A,
    paths: Option<Vec<FilePath>>,
    channel: Channel<AnswerUpload>,
) {
    enum ProcessingState {
        Starting,
        Finishing,
        Done(Vec<Result<(String, Mat, AnswerSheet), UploadError>>),
    }

    let Some(paths) = paths else {
        signal!(channel, AnswerUpload::Cancelled);
        return;
    };

    let images_count = paths.len();

    signal!(
        channel,
        AnswerUpload::Processing {
            total: images_count,
            started: 0,
            finished: 0
        }
    );

    let (tx, mut rx) = tauri::async_runtime::channel::<ProcessingState>(images_count);

    let processing_thread = tauri::async_runtime::spawn(async move {
        let base64_list: Vec<Result<(String, Mat, AnswerSheet), UploadError>> = paths
            .into_par_iter()
            .map_with(
                (tx.clone(), state::init_thread_tesseract()),
                |(tx, tess), file_path| {
                    _ = tx.try_send(ProcessingState::Starting);
                    let res = handle_upload(file_path, tess);
                    _ = tx.try_send(ProcessingState::Finishing);
                    res
                },
            )
            .collect();
        _ = tx.send(ProcessingState::Done(base64_list)).await;
    });

    let (mut started, mut finished) = (0usize, 0usize);

    loop {
        match rx.blocking_recv() {
            None => signal!(
                channel,
                AnswerUpload::Error {
                    error: format!("{}", UploadError::UnexpectedPipeClosure)
                }
            ),
            Some(state) => match state {
                ProcessingState::Starting => started += 1,
                ProcessingState::Finishing => finished += 1,
                ProcessingState::Done(list) => {
                    signal!(channel, AnswerUpload::AlmostDone);
                    AppState::upload_answer_sheets(app, channel, list);
                    processing_thread.abort();
                    break;
                }
            },
        }
        signal!(
            channel,
            AnswerUpload::Processing {
                total: images_count,
                started,
                finished
            }
        );
    }
}

pub fn resize_relative_img(src: &Mat, relative: f64) -> opencv::Result<Mat> {
    let mut dst = new_mat_copy!(src);
    let new_size = Size::new(
        (src.cols() as f64 * relative).round() as i32,
        (src.rows() as f64 * relative).round() as i32,
    );

    imgproc::resize(&src, &mut dst, new_size, 0.0, 0.0, imgproc::INTER_LINEAR)?;
    Ok(dst)
}

// fn show_img(mat: &Mat, window_name: &str) -> opencv::Result<()> {
//     println!("showing window {window_name}");
//     highgui::named_window(window_name, 0)?;
//     highgui::imshow(window_name, mat)?;
//     highgui::wait_key(10000)?;
//     Ok(())
// }

fn handle_upload(
    path: FilePath,
    tess: &TesseractAPI,
) -> Result<(String, Mat, AnswerSheet), UploadError> {
    let mat = read_from_path(path)?;
    let resized = resize_relative_img(&mat, 0.3333).map_err(UploadError::from)?;
    let resized_for_fix = resized.clone();
    let (aligned_for_display, subject_id, student_id, answer_sheet) =
        fix_answer_sheet(resized_for_fix)?;

    let subject_id_string = extract_digits_for_sub_stu(&subject_id, 2, false)?;
    let student_id_string = extract_digits_for_sub_stu(&student_id, 9, true)?;
    println!("subject_id: {subject_id_string}");
    println!("subject_id: {student_id_string}");
    //testing
    //#[cfg(not(test))]
    //let _ = show_img(&aligned_for_processing, "resized & aligned image");
    let (name, subject, date, exam_room, seat) = extract_user_information(&resized, tess)?;
    println!("name: {name}");
    println!("name: {subject}");
    println!("name: {date}");
    println!("name: {exam_room}");
    println!("name: {seat}");
    let base64 = mat_to_base64_png(&aligned_for_display).map_err(UploadError::from)?;
    let answer_sheet: AnswerSheet = (subject_id, student_id, answer_sheet).try_into()?;
    Ok((base64, aligned_for_display, answer_sheet))
}

pub fn mat_to_base64_png(mat: &Mat) -> Result<String, opencv::Error> {
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
        .roi(Rect_ {
            x: 38,
            y: 30,
            width: 1095,
            height: 765,
        })?
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

fn extract_answers(answer_mat: &Mat) -> Result<[QuestionGroup; 36], SheetError> {
    // let mat_debug_cloned = answer_mat.try_clone()?;
    // let mut mat_debug = new_mat_copy!(answer_mat);
    // imgproc::cvt_color_def(&mat_debug_cloned, &mut mat_debug, COLOR_GRAY2RGB)?;
    let mut out = Vec::with_capacity(36);
    for x_idx in 0..4 {
        for y_idx in 0..9 {
            let (x, y) = (
                (ANSWER_WIDTH + ANSWER_WIDTH_GAP) * x_idx,
                (ANSWER_HEIGHT + ANSWER_HEIGHT_GAP) * y_idx,
            );
            let (x, y) = (
                x.clamp(0, answer_mat.cols() - ANSWER_WIDTH),
                y.clamp(0, answer_mat.rows() - ANSWER_HEIGHT),
            );
            let rect = Rect_ {
                x,
                y,
                width: ANSWER_WIDTH,
                height: ANSWER_HEIGHT,
            };
            // println!("block ({x_idx}, {y_idx}) at ({x}, {y})");
            // imgproc::rectangle_def(&mut mat_debug, rect, (255, 0, 0).into())?;
            let answers: Result<Vec<Option<Answer>>, SheetError> = (0..5)
                .map(|row_idx| {
                    let row_y = y
                        + ((ANSWER_HEIGHT / 5) * row_idx).clamp(0, rect.height - ANSWER_HEIGHT / 5);
                    let row_rect = Rect_ {
                        x: x + 24,
                        y: row_y,
                        width: ANSWER_WIDTH - 24,
                        height: ANSWER_HEIGHT / 5,
                    };
                    // imgproc::rectangle_def(&mut mat_debug, row_rect, (0, 0, 255).into())?;
                    let bubbles: Result<Vec<(u8, f32)>, SheetError> = (0u8..13u8)
                        .map(|bubble_idx| {
                            let bubble_x = (x + 24)
                                + ((row_rect.width / 12) * bubble_idx as i32)
                                    .clamp(0, row_rect.width - (row_rect.width / 13));
                            let bubble_rect = Rect_ {
                                x: bubble_x,
                                y: row_y,
                                width: row_rect.width / 13,
                                height: ANSWER_HEIGHT / 5,
                            };
                            let bubble_filled: u16 = answer_mat
                                .roi(bubble_rect)?
                                .clone_pointee()
                                .data_bytes()?
                                .iter()
                                .map(|n| *n as u16)
                                .sum();
                            let frac = bubble_filled as f32 / u16::MAX as f32;
                            // if frac < 0.45 {
                            //     imgproc::rectangle_def(
                            //         &mut mat_debug,
                            //         bubble_rect,
                            //         (255, 0, 255).into(),
                            //     )?;
                            // }
                            Ok((bubble_idx, frac))
                        })
                        .collect();
                    let bubbles = bubbles?;
                    let circled_in: Vec<u8> = bubbles
                        .into_iter()
                        .sorted_by(|&(_, a), &(_, b)| a.total_cmp(&b))
                        .filter_map(|(idx, f)| if f < 0.45 { Some(idx) } else { None })
                        .collect();
                    Ok(Answer::from_bubbles_vec(circled_in))
                })
                .collect();
            let answers: QuestionGroup = answers?.try_into()?;
            out.push(answers);
        }
        // imgcodecs::imwrite_def("debug-images/answer_borders.png", &mat_debug)?;
    }
    let mut out = out.into_iter();

    Ok(array::from_fn(|_| {
        out.next().expect("should have exactly 36 groups")
    }))
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

    let rows = if is_student_id { 8 } else { 2 };
    let cols = 10;
    let mut v: Vec<Vec<i32>> = vec![vec![0; cols]; rows];

    let mut digits = String::new();

    for i in 0..num_digits as usize {
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

        for j in 0..10usize {
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
                    v[i - 1][j] = sum as i32; //Skip first Index NaKub
                }
            } else {
                v[i][j] = sum as i32;
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
        println!("Row {i}: {row:?}");
    }
    Ok(digits)
}

impl TryFrom<(Mat, Mat, Mat)> for AnswerSheet {
    type Error = SheetError;
    fn try_from(value: (Mat, Mat, Mat)) -> Result<Self, Self::Error> {
        let (subject_code_mat, student_id_mat, answers_mat) = value;
        let subject_id_string = extract_digits_for_sub_stu(&subject_code_mat, 2, false)?;
        let student_id_string = extract_digits_for_sub_stu(&student_id_mat, 9, true)?;
        let scanned_answers = extract_answers(&answers_mat)?;

        // println!("subject_id: {subject_id_string}");
        // println!("subject_id: {student_id_string}");

        Ok(Self {
            subject_code: subject_id_string,
            student_id: student_id_string,
            answers: scanned_answers,
        })
    }
}

impl AnswerSheetResult {
    pub fn write_score_marks(&self, sheet: &mut Mat) -> Result<(), SheetError> {
        let mut sheet_transparent = new_mat_copy!(sheet);
        imgproc::cvt_color_def(sheet, &mut sheet_transparent, COLOR_GRAY2RGBA)?;
        *sheet = sheet_transparent.clone();
        for x_idx in 0..4 {
            for y_idx in 0..9 {
                let (x, y) = (
                    206 + (ANSWER_WIDTH + ANSWER_WIDTH_GAP) * x_idx,
                    14 + (ANSWER_HEIGHT + ANSWER_HEIGHT_GAP) * y_idx,
                );
                let (x, y) = (
                    x.clamp(0, sheet.cols() - ANSWER_WIDTH),
                    y.clamp(0, sheet.rows() - ANSWER_HEIGHT),
                );
                let rect = Rect_ {
                    x,
                    y,
                    width: ANSWER_WIDTH,
                    height: ANSWER_HEIGHT,
                };
                for row_idx in 0..5usize {
                    let result_here =
                        self.graded_questions[(x_idx * 9 + y_idx) as usize].at(row_idx);
                    let row_y = y
                        + ((ANSWER_HEIGHT / 5) * row_idx as i32)
                            .clamp(0, rect.height - ANSWER_HEIGHT / 5);
                    let row_rect = Rect_ {
                        x: x + 24,
                        y: row_y,
                        width: ANSWER_WIDTH - 24,
                        height: ANSWER_HEIGHT / 5,
                    };
                    let color: Option<opencv::core::Scalar> = result_here.and_then(|c| match c {
                        CheckedAnswer::Correct => Some((43, 160, 64).into()),
                        CheckedAnswer::Incorrect => Some((57, 15, 210).into()),
                        CheckedAnswer::Missing => Some((29, 142, 223).into()),
                        CheckedAnswer::NotCounted => None,
                    });
                    if let Some(color) = color {
                        imgproc::rectangle(
                            &mut sheet_transparent,
                            row_rect,
                            color,
                            FILLED,
                            LINE_8,
                            0,
                        )?;
                    }
                }
            }
        }
        let mut res = new_mat_copy!(sheet);
        opencv::core::add_weighted_def(
            &sheet_transparent,
            MARKER_TRANSPARENCY,
            sheet,
            1.0 - MARKER_TRANSPARENCY,
            0.0,
            &mut res,
        )?;
        *sheet = res;
        Ok(())
    }
}
fn crop_user_information(mat: &Mat) -> Result<Mat, SheetError> {
    let user_information = mat
        .roi(Rect_ {
            x: 0,
            y: 92,
            width: 200,
            height: 90,
        })?
        .clone_pointee();
    Ok(user_information)
}

fn crop_each_part(mat: &Mat) -> Result<(Mat, Mat, Mat, Mat, Mat), SheetError> {
    let name = mat
        .roi(Rect_ {
            x: 45,
            y: 0,
            width: 150,
            height: 17,
        })?
        .clone_pointee();
    let subject = mat
        .roi(Rect_ {
            x: 21,
            y: 30,
            width: 176,
            height: 14,
        })?
        .clone_pointee();
    let date = mat
        .roi(Rect_ {
            x: 95,
            y: 49,
            width: 102,
            height: 18,
        })?
        .clone_pointee();
    let exam_room = mat
        .roi(Rect_ {
            x: 41,
            y: 71,
            width: 60,
            height: 18,
        })?
        .clone_pointee();
    let seat = mat
        .roi(Rect_ {
            x: 152,
            y: 72,
            width: 45,
            height: 17,
        })?
        .clone_pointee();

    Ok((name, subject, date, exam_room, seat))
}

fn image_to_string(mat: &Mat, tesseract: &TesseractAPI) -> Result<String, SheetError> {
    let width = mat.cols();
    let height = mat.rows();
    let bytes_per_pixel = 1;
    let bytes_per_line = width;

    //println!("is_continuous: {}", mat.is_continuous());

    let image_data = mat.data_bytes()?;

    tesseract.set_image(image_data, width, height, bytes_per_pixel, bytes_per_line)?;

    let text = tesseract.get_utf8_text()?;

    Ok(text.trim().to_string())
}

fn extract_user_information(
    mat: &Mat,
    tess: &TesseractAPI,
) -> Result<(String, String, String, String, String), SheetError> {
    let temp_dir = "temp";
    _ = fs::create_dir_all(temp_dir);

    println!("Working directory: {:?}", std::env::current_dir());

    let user_information = crop_user_information(mat)?;
    let (name, subject, date, exam_room, seat) = crop_each_part(&user_information)?;

    if cfg!(debug_assertions) {
        safe_imwrite("temp/debug_name.png", &name)?;
        safe_imwrite("temp/debug_subject.png", &subject)?;
        safe_imwrite("temp/debug_date.png", &date)?;
        safe_imwrite("temp/debug_exam_room.png", &exam_room)?;
        safe_imwrite("temp/debug_seat.png", &seat)?;
    }

    let name_string = image_to_string(&name, tess)?;
    let subject_string = image_to_string(&subject, tess)?;
    let date_string = image_to_string(&date, tess)?;
    let exam_room_string = image_to_string(&exam_room, tess)?;
    let seat_string = image_to_string(&seat, tess)?;

    Ok((
        name_string,
        subject_string,
        date_string,
        exam_room_string,
        seat_string,
    ))
}

fn safe_imwrite<P: AsRef<Path>>(path: P, mat: &Mat) -> Result<bool, opencv::Error> {
    if mat.empty() {
        println!(
            "Warning: attempting to write an empty Mat to {:?}",
            path.as_ref()
        );
    } else {
        println!("Writing debug image to {:?}", path.as_ref());
    }
    imgcodecs::imwrite(
        path.as_ref().to_str().unwrap(),
        mat,
        &opencv::core::Vector::new(),
    )
}

#[cfg(test)]
mod unit_tests {
    use std::path::PathBuf;

    use crate::state;

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

    fn setup_tessdata() {
        state::init_tessdata(PathBuf::from("tests/assets"));
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
        setup_tessdata();
        let path = test_key_image();
        let result = handle_upload(path, &state::init_thread_tesseract());
        assert!(result.is_ok());

        let (base64_string, mat, _answer_sheet) = result.unwrap();
        assert!(base64_string.starts_with("data:image/png;base64,"));
        assert!(!mat.empty());
    }

    #[test]
    fn test_handle_upload_failure() {
        setup_tessdata();
        let path = not_image();
        let result = handle_upload(path, &state::init_thread_tesseract());
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

        let resized = resize_relative_img(&mat, 0.333);
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
        let resized = resize_relative_img(&mat, 0.3333).expect("Resize failed");
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
