use crate::err_log;
use crate::ocr::{ImageSource, OcrEngine};
use log::{debug, warn};
use std::ops::RangeInclusive;
use std::sync::{Arc, RwLock};
use std::{array, mem};
use tauri::ipc::Channel;

use crate::errors::{SheetError, UploadError};
use crate::scoring::{AnswerSheetResult, CheckedAnswer};
use crate::{signal, state};
use itertools::Itertools;
use opencv::{
    boxed_ref::BoxedRef,
    core::{Mat, MatTraitConstManual, Moments, Point, Rect2i, Rect_, Size, ToInputArray, Vector},
    imgcodecs::{self, imread, ImreadModes},
    imgproc,
    prelude::*,
};
use rayon::prelude::*;
use std::path::Path;
use tauri_plugin_dialog::FilePath;

use tauri::{Emitter, Manager, Runtime};

use crate::state::{
    Answer, AnswerSheet, AnswerUpload, AppState, KeyUpload, Options, QuestionGroup,
};

/// Creates a new **uninitialized!!!!** `Mat` with the same dimensions as the argument.
/// generally okay to pass into the out param field (usually called `dst`) in opencv functions.
/// however, trying to access this `Mat` (usually with something like `copy_to()`) without initializing **WILL** segfault.
/// if you want to initialize, use `Mat::new_rows_cols_with_default` and friends.
macro_rules! new_mat_copy {
    ($orig: ident) => {{
        let mut mat = Mat::default();
        mat.set_rows($orig.rows());
        mat.set_cols($orig.cols());
        mat
    }};
}

struct SplittedSheet {
    original: Mat,

    student_name: Mat,
    subject_name: Mat,
    exam_room: Mat,
    exam_seat: Mat,

    subject_id: Mat,
    student_id: Mat,

    questions: Vec<Mat>,
}

pub fn upload_key_image_impl<R: Runtime, A: Emitter<R> + Manager<R>>(
    app: &A,
    path_maybe: Option<FilePath>,
    channel: Channel<KeyUpload>,
) {
    let Some(file_path) = path_maybe else {
        signal!(channel, KeyUpload::Cancelled);
        return;
    };
    let Options { ocr, mongo: _ } = AppState::get_options(app);
    match handle_upload(
        file_path,
        ocr.then(state::init_thread_ocr).flatten().as_ref(),
    ) {
        Ok((image, mat, key)) => AppState::upload_key(app, channel, image, mat, key.into()),
        Err(e) => {
            err_log!(&e);
            signal!(
                channel,
                KeyUpload::Error {
                    error: format!("{e}")
                }
            )
        }
    }
}

pub type ResultOfImageMatSheet = Result<(Vec<u8>, Mat, AnswerSheet), UploadError>;

pub enum ProcessingState {
    Starting,
    Finishing,
    Cancel,
    Done(Vec<ResultOfImageMatSheet>),
}
pub fn upload_sheet_images_impl<R: Runtime, A: Emitter<R> + Manager<R>>(
    app: &A,
    paths: Option<Vec<FilePath>>,
    channel: Channel<AnswerUpload>,
) {
    let Some(paths) = paths else {
        signal!(channel, AnswerUpload::Cancelled);
        return;
    };

    let images_count = paths.len();
    let Options { ocr, mongo: _ } = AppState::get_options(app);

    let (tx, mut rx) = tauri::async_runtime::channel::<ProcessingState>(images_count);
    let stop_flag = Arc::new(RwLock::new(false));

    AppState::mark_scoring(app, &channel, images_count, tx.clone());

    let stop_moved = Arc::clone(&stop_flag);
    let processing_thread = tauri::async_runtime::spawn(async move {
        let base64_list: Vec<ResultOfImageMatSheet> = paths
            .into_par_iter()
            .map_init(
                || {
                    (
                        tx.clone(),
                        ocr.then(state::init_thread_ocr).flatten(),
                        Arc::clone(&stop_moved),
                    )
                },
                |(tx, ocr, stop), file_path| {
                    if !*stop.read().expect("not poisoned") {
                        _ = tx.try_send(ProcessingState::Starting);
                        let res = handle_upload(file_path, ocr.as_ref());
                        _ = tx.try_send(ProcessingState::Finishing);
                        res
                    } else {
                        Err(UploadError::PrematureCancellaton)
                    }
                },
            )
            .collect();
        _ = tx.send(ProcessingState::Done(base64_list)).await;
    });

    let (mut started, mut finished) = (0usize, 0usize);

    loop {
        match rx.blocking_recv() {
            None => {
                err_log!(&UploadError::UnexpectedPipeClosure);
                signal!(
                    channel,
                    AnswerUpload::Error {
                        error: format!("{}", UploadError::UnexpectedPipeClosure)
                    }
                )
            }
            Some(state) => match state {
                ProcessingState::Starting => started += 1,
                ProcessingState::Finishing => finished += 1,
                ProcessingState::Done(list) => {
                    AppState::upload_answer_sheets(app, &channel, list);
                    processing_thread.abort();
                    break;
                }
                ProcessingState::Cancel => {
                    *stop_flag.write().expect("not poisoned") = true;
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
    let mut dst = Mat::default();
    let new_size = Size::new(
        (src.cols() as f64 * relative).round() as i32,
        (src.rows() as f64 * relative).round() as i32,
    );

    imgproc::resize(&src, &mut dst, new_size, 0.0, 0.0, imgproc::INTER_LINEAR)?;
    Ok(dst)
}

fn handle_upload(
    path: FilePath,
    ocr: Option<&OcrEngine>,
) -> Result<(Vec<u8>, Mat, AnswerSheet), UploadError> {
    let mat = read_from_path(path)?;
    let mut splitted = prepare_answer_sheet(mat)?;

    let original = mem::take(&mut splitted.original);
    let bytes = mat_to_webp(&original).map_err(UploadError::from)?;
    let answer_sheet = AnswerSheet::try_convert(splitted, ocr)?;
    Ok((bytes, original, answer_sheet))
}

pub fn mat_to_webp(mat: &Mat) -> opencv::Result<Vec<u8>> {
    let mut buf: Vector<u8> = Vec::new().into();
    imgcodecs::imencode(
        ".webp",
        mat,
        &mut buf,
        &vec![imgcodecs::IMWRITE_WEBP_QUALITY, 80].into(),
    )?;
    Ok(buf.into())
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

fn crop_to_markers(mat: Mat) -> Result<Mat, SheetError> {
    // we force drop the original `mat` here by capturing it in the closure - lmk if you have a cleaner way
    #[allow(clippy::redundant_closure_call)]
    let mat = {
        #[inline(always)]
        move || roi_range_frac(&mat, 0.00570288..=0.99714856, 0.008064516..=0.995967742)
    }()?;
    let blurred_thresholded = {
        let mut blur = new_mat_copy!(mat);
        imgproc::gaussian_blur_def(&mat, &mut blur, (5, 5).into(), 0.0)?;
        // SAFETY: adaptive_threshold can operate in place.
        unsafe {
            blur.modify_inplace(|blurred, thresholded| {
                imgproc::adaptive_threshold(
                    blurred,
                    thresholded,
                    255.0,
                    imgproc::ADAPTIVE_THRESH_GAUSSIAN_C,
                    imgproc::THRESH_BINARY_INV,
                    11,
                    2.0,
                )
            })?;
        }
        blur
    };

    let contours = {
        let mut vec: Vector<Vector<Point>> = vec![].into();
        imgproc::find_contours_def(
            &blurred_thresholded,
            &mut vec,
            imgproc::RETR_EXTERNAL,
            imgproc::CHAIN_APPROX_SIMPLE,
        )?;
        vec
    };

    let mut corners: (Option<Point>, (Option<i32>, Option<i32>)) = (None, (None, None));
    for contour in contours {
        let length_approx = imgproc::arc_length(&contour, true)?;
        let epsilon = 0.04 * length_approx;

        let mut approx: Vector<Point> = vec![].into();
        imgproc::approx_poly_dp(&contour, &mut approx, epsilon, true)?;

        // Check if it's a triangle
        if length_approx > 90.0 && approx.len() == 3 {
            let Moments { m00, m01, m10, .. } = imgproc::moments_def(&contour)?;
            let cx = (m10 / m00) as i32;
            let cy = (m01 / m00) as i32;

            if cx + cy < 300 {
                _ = corners.0.replace((cx, cy).into());
            }
            if cx > 1000 {
                _ = corners.1 .0.replace(cx);
            } else if cy > 1000 {
                _ = corners.1 .1.replace(cy);
            }
        }
    }

    let (Some(Point { x, y }), (Some(x2), Some(y2))) = corners else {
        return Err(SheetError::MissingMarkers);
    };

    let cropped = mat
        .roi(Rect_ {
            x,
            y,
            width: x2 - x,
            height: y2 - y,
        })?
        .clone_pointee();

    Ok(cropped)
}

fn prepare_answer_sheet(mat: Mat) -> Result<SplittedSheet, SheetError> {
    let cropped = crop_to_markers(mat)?;
    // #[cfg(test)]
    // {
    //     safe_imwrite("temp/cropped.png", &cropped)?;
    // }
    let splitted = split_into_areas(cropped)?;
    Ok(splitted)
}

fn rect_range_frac(rect: &Rect2i, x: RangeInclusive<f64>, y: RangeInclusive<f64>) -> Rect2i {
    let (range_x, range_y) = (x.end() - x.start(), y.end() - y.start());
    let (width, height) = (rect.width, rect.height);
    let (start_x, start_y) = (width as f64 * x.start(), height as f64 * y.start());
    let (range_width, range_height) = (width as f64 * range_x, height as f64 * range_y);
    Rect_ {
        x: rect.x + start_x as i32,
        y: rect.y + start_y as i32,
        width: range_width as i32,
        height: range_height as i32,
    }
}
fn roi_range_frac_ref(
    mat: &impl MatTraitConst,
    x: RangeInclusive<f64>,
    y: RangeInclusive<f64>,
) -> opencv::Result<BoxedRef<'_, Mat>> {
    let rect = Rect_::from_point_size((0, 0).into(), mat.size()?);
    mat.roi(rect_range_frac(&rect, x, y))
}
fn roi_range_frac(
    mat: &impl MatTraitConst,
    x: RangeInclusive<f64>,
    y: RangeInclusive<f64>,
) -> opencv::Result<Mat> {
    roi_range_frac_ref(mat, x, y).map(|ok| ok.clone_pointee())
}

fn thresh(mut mat: Mat) -> opencv::Result<Mat> {
    // SAFETY: threshold can operate in place.
    unsafe {
        mat.modify_inplace(|mat, thresholded| {
            imgproc::threshold(mat, thresholded, 165.0, 255.0, imgproc::THRESH_BINARY)
        })?;
    }
    Ok(mat)
}
const START_PERCENT_X: f64 = 0.18525022;
const START_PERCENT_Y: f64 = 0.010113780;
const WIDTH_PERCENT: f64 = 0.19841967;
const HEIGHT_PERCENT: f64 = 0.094816688;
const GAP_X_PERCENT: f64 = 0.0079016681;
const GAP_Y_PERCENT: f64 = 0.015170670;
fn split_into_areas(sheet: Mat) -> Result<SplittedSheet, SheetError> {
    let subject_name = roi_range_frac(&sheet, 0.01317..=0.1765, 0.1479..=0.1656)?;
    let student_name = roi_range_frac(&sheet, 0.0342..=0.1773, 0.1113..=0.1340)?;
    let exam_room = roi_range_frac(&sheet, 0.032484636..=0.088674276, 0.206068268..=0.230088496)?;
    let exam_seat = roi_range_frac(&sheet, 0.134328358..=0.175592625, 0.206068268..=0.230088496)?;

    let sheet = {
        #[inline(always)]
        move || resize_relative_img(&sheet, 0.3333)
    }()?;
    let original = sheet.clone();

    let subject_id = roi_range_frac(&sheet, 0.0..=0.040386304, 0.271807838..=0.517067004)?;
    let student_id = roi_range_frac(&sheet, 0.049165935..=0.177348551, 0.273072061..=0.515802781)?;
    let questions: Vec<Mat> = {
        let ranges = (0..4).flat_map(|x| (0..9).map(move |y| (x, y)));
        ranges
            .map(|(x, y)| {
                let min_x = START_PERCENT_X + x as f64 * (GAP_X_PERCENT + WIDTH_PERCENT);
                let max_x = f64::min(min_x + WIDTH_PERCENT, 1.0);
                let min_y = START_PERCENT_Y + y as f64 * (GAP_Y_PERCENT + HEIGHT_PERCENT);
                let max_y = f64::min(min_y + HEIGHT_PERCENT, 1.0);
                thresh(roi_range_frac(&sheet, min_x..=max_x, min_y..=max_y)?)
            })
            .collect::<opencv::Result<Vec<Mat>>>()?
    };

    Ok(SplittedSheet {
        original,
        student_name,
        subject_name,
        exam_room,
        exam_seat,
        subject_id,
        student_id,
        questions,
    })
}

fn sorted_bubbles_by_filled<Src: Iterator<Item = Mat>>(
    src: Src,
) -> impl Iterator<Item = (usize, f32)> {
    src.enumerate()
        .map(|(idx, bubble)| {
            let max_white = u8::MAX as u32 * (bubble.cols() * bubble.rows()) as u32;
            let bubble_sum: u32 = bubble
                .data_bytes()
                .expect("Mat is not continuous")
                .iter()
                .copied()
                .map(|p| p as u32)
                .sum();

            (idx, 1.0 - (bubble_sum as f32 / max_white as f32))
        })
        .sorted_by(|a, b| PartialOrd::partial_cmp(&b.1, &a.1).expect("not NaN"))
}

fn extract_answers(answer_mats: Vec<Mat>) -> Result<[QuestionGroup; 36], SheetError> {
    let mut out = answer_mats
        .into_iter()
        .map(|mat| {
            let mut iter = (0..5).map(|row_idx| {
                let row = roi_range_frac_ref(
                    &mat,
                    0.11946903..=1.0,
                    (row_idx as f64 / 5.0)..=(row_idx as f64 + 1.0) / 5.0,
                )?;
                Result::<_, opencv::Error>::Ok(
                    sorted_bubbles_by_filled((0..13u8).filter_map(move |bubble_idx| {
                        roi_range_frac(
                            &row,
                            bubble_idx as f64 / 13.0..=(bubble_idx as f64 + 1.0) / 13.0,
                            0.0..=1.0,
                        )
                        .inspect_err(|e| err_log!(e))
                        .ok()
                    }))
                    .filter_map(|(idx, filled)| (filled > 0.4).then_some(idx as u8)),
                )
            });
            Ok(QuestionGroup {
                A: Answer::from_bubbles_iter(iter.next().expect("5 rows")?),
                B: Answer::from_bubbles_iter(iter.next().expect("5 rows")?),
                C: Answer::from_bubbles_iter(iter.next().expect("5 rows")?),
                D: Answer::from_bubbles_iter(iter.next().expect("5 rows")?),
                E: Answer::from_bubbles_iter(iter.next().expect("5 rows")?),
            })
        })
        .collect::<Result<Vec<_>, opencv::Error>>()?
        .into_iter();

    Ok(array::from_fn(|_| {
        out.next().expect("should have exactly 36 groups")
    }))
}

/// Note: the mat passed into this function has to be just the bubble columns, nothing on top
fn extract_digits_for_sub_stu<M: MatTraitConst + ToInputArray>(
    mat: &M,
    columns: u8,
) -> Result<String, opencv::Error> {
    let mut digits = String::new();

    for column_idx in 0..columns {
        let frac = column_idx as f64 / columns as f64;
        let next_frac = (column_idx as f64 + 1.0) / columns as f64;
        let column = roi_range_frac_ref(mat, frac..=next_frac, 0.0..=1.0)?;
        let circled = sorted_bubbles_by_filled((0..10).filter_map(|row_idx| {
            let frac = row_idx as f64 / 10.0;
            let next_frac = (row_idx as f64 + 1.0) / 10.0;
            roi_range_frac(&column, 0.0..=1.0, frac..=next_frac)
                .inspect_err(|e| err_log!(e))
                .ok()
                .and_then(|mat| thresh(mat).inspect_err(|e| err_log!(e)).ok())
        }))
        .find(|(_, filled)| *filled > 0.475)
        .map(|(idx, _)| idx);
        if let Some(circled) = circled {
            digits.push_str(&circled.to_string());
        }
    }
    Ok(digits)
}

impl AnswerSheet {
    fn try_convert(src: SplittedSheet, ocr: Option<&OcrEngine>) -> Result<Self, SheetError> {
        let SplittedSheet {
            student_name: student_name_mat,
            subject_name: subject_name_mat,
            exam_room: exam_room_mat,
            exam_seat: exam_seat_mat,
            subject_id: subject_id_mat,
            student_id: student_id_mat,
            questions,
            ..
        } = src;

        let subject_id_bubbles = roi_range_frac_ref(&subject_id_mat, 0.0..=1.0, 0.128205..=1.0)?;
        let student_id_bubbles = roi_range_frac_ref(&student_id_mat, 0.0..=1.0, 0.12565445..=1.0)?;

        let subject_id = extract_digits_for_sub_stu(&subject_id_bubbles, 3)?;
        let mut student_id = extract_digits_for_sub_stu(&student_id_bubbles, 9)?;
        let answers = extract_answers(questions)?;

        let (mut student_name, mut subject_name, mut exam_room, mut exam_seat) =
            (None, None, None, None);
        if let Some(ocr) = ocr {
            let subject_id_written = roi_range_frac(&subject_id_mat, 0.0..=1.0, 0.0..=0.128205)?;
            let student_id_written =
                roi_range_frac(&student_id_mat, 0.112..=1.0, 0.0..=0.12565445)?;
            let (written_subject_id, written_student_id) =
                extract_subject_student_from_written_field(
                    subject_id_written,
                    student_id_written,
                    ocr,
                )?;
            if subject_id != written_subject_id || student_id != written_student_id {
                //warn!("{} != {} && {} != {}", written_student_id, student_id, written_subject_id, subject_id);
                if student_id.len() != 8 && written_student_id.len() == 8 {
                    student_id = written_student_id;
                }
                warn!("User Fon and Enter differently");
            }
            let (name, subject, room, seat) = extract_user_information(
                student_name_mat,
                subject_name_mat,
                exam_room_mat,
                exam_seat_mat,
                ocr,
            )?;
            _ = student_name.insert(name);
            _ = subject_name.insert(subject);
            _ = exam_room.insert(room);
            _ = exam_seat.insert(seat);
        }

        Ok(Self {
            subject_id,
            student_id,
            subject_name,
            student_name,
            exam_room,
            exam_seat,
            answers,
        })
    }
}

impl AnswerSheetResult {
    pub fn write_score_marks(&self, sheet: &mut Mat) -> Result<(), SheetError> {
        // SAFETY: `cvt_color_def` can be done in place.
        unsafe {
            sheet.modify_inplace(|i, out| {
                imgproc::cvt_color_def(i, out, imgproc::COLOR_GRAY2RGBA)
            })?;
        }
        let mut color_overlay = sheet.clone();

        const MARKER_TRANSPARENCY: f64 = 0.3;

        let question_mats = {
            let ranges = (0..4).flat_map(|x| (0..9).map(move |y| (x, y)));
            ranges
                .map(|(x, y)| {
                    let min_x = START_PERCENT_X + x as f64 * (GAP_X_PERCENT + WIDTH_PERCENT);
                    let max_x = f64::min(min_x + WIDTH_PERCENT, 1.0);
                    let min_y = START_PERCENT_Y + y as f64 * (GAP_Y_PERCENT + HEIGHT_PERCENT);
                    let max_y = f64::min(min_y + HEIGHT_PERCENT, 1.0);
                    let rect = Rect_::from_point_size((0, 0).into(), color_overlay.size()?);
                    Ok(rect_range_frac(&rect, min_x..=max_x, min_y..=max_y))
                })
                .collect::<opencv::Result<Vec<Rect2i>>>()?
        };
        let mut mats_and_checked = question_mats.into_iter().zip(self.graded_questions);
        mats_and_checked.try_for_each(|(question_rect, (checked, _))| {
            let question_numbers = rect_range_frac(&question_rect, 0.0..=0.11946903, 0.0..=1.0);
            let verdict = checked.verdict();
            let question_color: Option<opencv::core::Scalar> = match verdict {
                CheckedAnswer::Correct => Some((43, 160, 64).into()),
                CheckedAnswer::Incorrect => Some((57, 15, 210).into()),
                CheckedAnswer::Missing => Some((29, 142, 223).into()),
                CheckedAnswer::NotCounted => None,
            };
            if let Some(question_color) = question_color {
                imgproc::rectangle(
                    &mut color_overlay,
                    question_numbers,
                    question_color,
                    imgproc::FILLED,
                    imgproc::LINE_8,
                    0,
                )?;
            }

            let mut rows = (0..5).map(|row| {
                (
                    row as usize,
                    rect_range_frac(
                        &question_rect,
                        0.11946903..=1.0,
                        (row as f64 / 5.0)..=((row as f64 + 1.0) / 5.0),
                    ),
                )
            });
            rows.try_for_each(|(idx, rect)| {
                let verdict = checked.at(idx).expect("checked answer < 5");
                let question_color: Option<opencv::core::Scalar> = match verdict {
                    CheckedAnswer::Correct => Some((43, 160, 64).into()),
                    CheckedAnswer::Incorrect => Some((57, 15, 210).into()),
                    CheckedAnswer::Missing => Some((29, 142, 223).into()),
                    CheckedAnswer::NotCounted => None,
                };
                if let Some(question_color) = question_color {
                    imgproc::rectangle(
                        &mut color_overlay,
                        rect,
                        question_color,
                        imgproc::FILLED,
                        imgproc::LINE_8,
                        0,
                    )?;
                }
                Result::<(), SheetError>::Ok(())
            })?;

            Result::<(), SheetError>::Ok(())
        })?;

        let mut res = new_mat_copy!(sheet);
        opencv::core::add_weighted_def(
            &color_overlay,
            MARKER_TRANSPARENCY,
            sheet,
            1.0 - MARKER_TRANSPARENCY,
            0.0,
            &mut res,
        )?;
        imgproc::cvt_color_def(&res, sheet, imgproc::COLOR_RGBA2RGB)?;

        Ok(())
    }
}

fn image_to_string(mat: &Mat, ocr: &OcrEngine) -> Result<String, SheetError> {
    let bytes = mat.data_bytes()?;
    let img_src = ImageSource::from_bytes(bytes, (mat.cols() as u32, mat.rows() as u32))?;
    let ocr_input = ocr.prepare_input(img_src)?;
    let text = ocr.get_text(ocr_input)?;
    let rem_nl = text.lines().next().unwrap_or("").trim().to_string();

    Ok(rem_nl)
}

fn extract_user_information(
    name: Mat,
    subject_name: Mat,
    exam_room: Mat,
    exam_seat: Mat,
    ocr: &OcrEngine,
) -> Result<(String, String, String, String), SheetError> {
    // safe_imwrite("temp/debug_name.png", &name)?;
    // safe_imwrite("temp/debug_subject_name.png", &subject_name)?;
    // safe_imwrite("temp/debug_exam_room.png", &exam_room)?;
    // safe_imwrite("temp/debug_exam_seat.png", &exam_seat)?;

    let name_string = image_to_string(&name, ocr)?;
    let subject_string = image_to_string(&subject_name, ocr)?;
    let exam_room_string = image_to_string(&exam_room, ocr)?
        .chars()
        .filter_map(|c| match c {
            c if c.is_ascii_digit() => Some(c),
            'O' | 'o' => Some('0'),
            'l' | 'I' | '|' => Some('1'),
            'Z' => Some('2'),
            'S' => Some('5'),
            'G' => Some('6'),
            'B' => Some('8'),
            _ => None,
        })
        .collect::<String>();
    let seat_string = {
        let mut code: Option<char> = None;
        let mut number: u8 = 0;
        image_to_string(&exam_seat, ocr)?
            .chars()
            .filter(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            .enumerate()
            .for_each(|(i, c)| {
                if i == 0 {
                    _ = code.get_or_insert(match c {
                        '0' => 'O',
                        c => c,
                    });
                } else {
                    let num = match c {
                        'O' => 0,
                        'I' => 1,
                        'Z' => 2,
                        'S' => 5,
                        'G' => 6,
                        'B' => 8,
                        c if c.is_ascii_digit() => c.to_digit(10).unwrap() as u8,
                        _ => 0,
                    };
                    number = number * 10 + num;
                }
            });

        format!("{}{:02}", code.unwrap_or_default(), number)
    };

    Ok((name_string, subject_string, exam_room_string, seat_string))
}

#[cfg(any(test, debug_assertions))]
#[allow(dead_code)]
fn safe_imwrite<P: AsRef<Path>, M: MatTraitConst + ToInputArray>(
    path: P,
    mat: &M,
) -> Result<bool, opencv::Error> {
    if mat.empty() {
        warn!(
            "Warning: attempting to write an empty Mat to {:?}",
            path.as_ref()
        );
    } else {
        debug!("Writing debug image to {:?}", path.as_ref());
    }
    imgcodecs::imwrite(
        path.as_ref().to_str().unwrap(),
        mat,
        &opencv::core::Vector::new(),
    )
}

fn extract_subject_student_from_written_field(
    subject_id_mat: Mat,
    student_id_mat: Mat,
    ocr: &OcrEngine,
) -> Result<(String, String), SheetError> {
    // safe_imwrite("temp/debug_subject_r.png", &subject_id_mat)?;
    // safe_imwrite("temp/debug_student_r.png", &student_id_mat)?;

    let rsub = image_to_string(&subject_id_mat, ocr)?;
    let rstu = image_to_string(&student_id_mat, ocr)?;

    let subject = clean_text(&rsub);
    let student = clean_text(&rstu);

    Ok((subject, student))
}

fn clean_text(raw: &str) -> String {
    raw.chars()
        .filter_map(|c| {
            if c == 'o' || c == 'O' {
                Some('0')
            } else if c.is_ascii_digit() {
                Some(c)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod unit_tests {
    use std::path::PathBuf;

    use crate::state;

    use super::*;
    use itertools::izip;
    use opencv::core;

    fn test_key_image() -> FilePath {
        FilePath::Path(PathBuf::from("tests/assets/sample_valid_image.jpg"))
    }

    fn test_images() -> Vec<FilePath> {
        vec![
            FilePath::Path(PathBuf::from("tests/assets/image_001.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/image_002.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/image_003.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/image_004.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/scan1_001.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/scan1_002.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/scan1_003.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/scan2_001.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/scan2_002.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/scan2_003.jpg")),
        ]
    }

    fn setup_ocr_data() {
        _ = state::MODELS.set(PathBuf::from("tests/assets"))
    }

    fn not_image() -> FilePath {
        FilePath::Path(PathBuf::from("tests/assets/sample_invalid_image.jpg"))
    }

    #[test]
    fn test_basic_functionality() {
        // Create a 2x2 black image (3 channels, 8-bit)
        let mat =
            Mat::new_rows_cols_with_default(2, 2, core::CV_8UC3, core::Scalar::all(0.0)).unwrap();

        let result = mat_to_webp(&mat);
        assert!(result.is_ok());

        let data = result.unwrap();

        // WEBP signature bytes
        assert_eq!(&data[0..4], b"RIFF");
        assert_eq!(&data[8..12], b"WEBP");
    }

    #[test]
    fn test_empty_mat_should_fail() {
        // Create an empty Mat
        let mat = Mat::default();
        let result = mat_to_webp(&mat);
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
        setup_ocr_data();
        let path = test_key_image();
        let result = handle_upload(
            path,
            cfg!(feature = "ocr-tests")
                .then(state::init_thread_ocr)
                .flatten()
                .as_ref(),
        );
        assert!(result.is_ok());

        let (_image, mat, _answer_sheet) = result.unwrap();
        assert!(!mat.empty());
    }

    #[test]
    fn test_handle_upload_failure() {
        setup_ocr_data();
        let path = not_image();
        let result = handle_upload(
            path,
            cfg!(feature = "ocr-tests")
                .then(state::init_thread_ocr)
                .flatten()
                .as_ref(),
        );
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
    fn test_crop_to_markers_size() {
        let mat_markers = read_from_path(test_key_image()).unwrap();
        // let mat_no_markers = {
        //     let (cols, rows) = (mat_markers.rows() as usize, mat_markers.cols() as usize);
        //     let white = vec![255u8; cols * rows];
        //     let mat = Mat::from_bytes::<u8>(&white);
        //     mat.unwrap().clone_pointee()
        // };
        let cropped_ok = crop_to_markers(mat_markers);
        assert!(cropped_ok.is_ok());
        // let cropped_not_ok = crop_to_markers(mat_no_markers);
        // assert!(cropped_not_ok.is_err());
        // dbg!(&cropped_not_ok);
        // assert!(matches!(
        //     cropped_not_ok.unwrap_err(),
        //     SheetError::MissingMarkers
        // ));

        for image in test_images() {
            println!("testing image {image}");
            let mat = read_from_path(image).unwrap();
            let cropped = crop_to_markers(mat);
            assert!(cropped.is_ok());
        }
    }

    #[test]
    fn test_split_into_areas() {
        for image in test_images() {
            println!("testing image {image}");
            let mat = read_from_path(image).unwrap();
            let cropped = crop_to_markers(mat).unwrap();
            let splitted = split_into_areas(cropped);
            assert!(splitted.is_ok());
        }
    }

    fn extract_check_id(path: FilePath, subject_id_expected: &str, student_id_expected: &str) {
        let mat = read_from_path(path).expect("Failed to read image");
        let SplittedSheet {
            subject_id,
            student_id,
            ..
        } = prepare_answer_sheet(mat).expect("Fixing sheet failed");
        // safe_imwrite("temp/subject.png", &subject_id).unwrap();
        // safe_imwrite("temp/student.png", &student_id).unwrap();

        let subject_id_bubbles =
            roi_range_frac_ref(&subject_id, 0.0..=1.0, 0.128205..=1.0).unwrap();
        let student_id_bubbles =
            roi_range_frac_ref(&student_id, 0.0..=1.0, 0.12565445..=1.0).unwrap();

        let subject_id = extract_digits_for_sub_stu(&subject_id_bubbles, 3)
            .expect("Extracting subject ID failed");
        assert_eq!(
            subject_id, subject_id_expected,
            "Subject ID does not match expected value"
        );

        let student_id = extract_digits_for_sub_stu(&student_id_bubbles, 9)
            .expect("Extracting student ID failed");
        assert_eq!(
            student_id, student_id_expected,
            "Student ID does not match expected value"
        );
    }
    #[test]
    fn check_extracted_ids_from_real_image() {
        extract_check_id(test_key_image(), "10", "165010001");
    }
    #[test]
    fn check_all_extracted_ids_from_images() {
        let images = test_images();
        let subject_ids = ["10", "10", "10", "17", "10", "10", "10", "10", "10", "10"];
        let student_ids = [
            "165010002",
            "165010003",
            "165010004",
            "165010014",
            "68010000",
            "68010001",
            "68010002",
            "68010000",
            "68010001",
            "68010002",
        ];
        for (image, subject_id, student_id) in izip!(images, subject_ids, student_ids) {
            println!(
                "checking for subject '{subject_id}' and student '{student_id}' in sheet '{image}'"
            );
            extract_check_id(image, subject_id, student_id);
        }
    }

    #[test]
    fn check_all_bubbles_non_empty() {
        for image in test_images() {
            println!("checking sheet '{image}' if all questions are answered");
            let mat = read_from_path(image).expect("Failed to read image");
            let SplittedSheet { questions, .. } =
                prepare_answer_sheet(mat).expect("Fixing sheet failed");
            let questions = extract_answers(questions).expect("reading questions failed");
            let res = questions
                .into_iter()
                .map(|group| {
                    let a = group.A.is_some();
                    let b = group.B.is_some();
                    let c = group.C.is_some();
                    let d = group.D.is_some();
                    let e = group.E.is_some();
                    a || b || c || d || e
                })
                .reduce(|acc, res| acc || res);
            assert!(res.unwrap());
        }
    }

    #[cfg(feature = "ocr-tests")]
    mod ocr {
        use super::*;

        use crate::state;

        #[test]
        fn check_extracted_ids_ocr() -> Result<(), SheetError> {
            setup_ocr_data();
            let ocr = &state::init_thread_ocr().unwrap();

            for (i, path) in test_images().into_iter().take(3).enumerate() {
                let mat = read_from_path(path).expect("Failed to read image");
                let SplittedSheet {
                    subject_id,
                    student_id,
                    ..
                } = prepare_answer_sheet(mat).expect("Resize failed");
                let subject_id_written =
                    roi_range_frac(&subject_id, 0.0..=1.0, 0.0..=0.128205).unwrap();
                let student_id_written =
                    roi_range_frac(&student_id, 0.0..=1.0, 0.0..=0.12565445).unwrap();
                let (subject_id, student_id) = extract_subject_student_from_written_field(
                    subject_id_written,
                    student_id_written,
                    ocr,
                )?;

                if i == 0 {
                    assert_eq!(subject_id, "10", "Subject ID does not match expected value");
                    assert_eq!(
                        student_id, "65010002",
                        "Student ID does not match expected value"
                    );
                } else if i == 1 {
                    assert_eq!(subject_id, "10", "Subject ID does not match expected value");
                    assert_eq!(
                        student_id, "65010003",
                        "Student ID does not match expected value"
                    );
                } else {
                    assert_eq!(subject_id, "10", "Subject ID does not match expected value");
                    assert_eq!(
                        student_id, "65010004",
                        "Student ID does not match expected value"
                    );
                }
            }
            Ok(())
        }

        #[test]
        fn check_ocr_function() -> Result<(), SheetError> {
            setup_ocr_data();
            let ocr = &state::init_thread_ocr().unwrap();

            for (i, path) in test_images().into_iter().take(3).enumerate() {
                println!("image #{i}");
                let mat = read_from_path(path).expect("Failed to read image");
                let SplittedSheet {
                    student_name,
                    subject_name,
                    exam_room,
                    exam_seat,
                    ..
                } = prepare_answer_sheet(mat).unwrap();

                let (name, subject, exam_room, seat) = extract_user_information(
                    student_name,
                    subject_name,
                    exam_room,
                    exam_seat,
                    ocr,
                )?;
                if i == 0 {
                    assert_eq!(name, "Elize Howells", "Name does not match expected value");
                    assert_eq!(
                        subject, "Mathematics",
                        "Subject does not match expected value"
                    );
                    assert_eq!(exam_room, "608", "Exam room does not match expected value");
                    assert_eq!(seat, "A02", "Seat does not match expected value");
                } else if i == 1 {
                    assert_eq!(name, "Marcia Cole", "Name does not match expected value");
                    assert_eq!(
                        subject, "Mathematics",
                        "Subject does not match expected value"
                    );
                    assert_eq!(exam_room, "608", "Exam room does not match expected value");
                    assert_eq!(seat, "A03", "Seat does not match expected value");
                } else {
                    assert_eq!(
                        name, "Sophie—Louise Green",
                        "Name does not match expected value"
                    );
                    assert_eq!(
                        subject, "Mathematics",
                        "Subject does not match expected value"
                    );
                    assert_eq!(exam_room, "608", "Exam room does not match expected value");
                    assert_eq!(seat, "A04", "Seat does not match expected value");
                }
            }

            Ok(())
        }
    }
}
