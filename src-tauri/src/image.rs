use crate::err_log;
use log::{debug, warn};
use ocrs::{ImageSource, OcrEngine};
use std::ops::RangeInclusive;
use std::sync::{Arc, RwLock};
use std::{array, mem};
use tauri::ipc::Channel;

use crate::errors::{SheetError, UploadError};
use crate::scoring::{AnswerSheetResult, CheckedAnswer};
use crate::{signal, state};
use base64::Engine;
use itertools::Itertools;
use opencv::core::{Mat, Moments, Point, Rect_, Size, Vector};
use opencv::imgproc::{COLOR_GRAY2RGBA, FILLED, LINE_8};
use opencv::{core::MatTraitConstManual, imgcodecs, imgproc, prelude::*};
use rayon::prelude::*;
use std::path::Path;
use tauri_plugin_dialog::FilePath;

use tauri::{Emitter, Manager, Runtime};

use opencv::imgcodecs::{imencode, imread, ImreadModes};

use crate::state::{
    Answer, AnswerSheet, AnswerUpload, AppState, KeyUpload, Options, QuestionGroup,
};

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
        Ok((base64_image, mat, key)) => {
            AppState::upload_key(app, channel, base64_image, mat, key.into())
        }
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

pub enum ProcessingState {
    Starting,
    Finishing,
    Cancel,
    Done(Vec<Result<(String, Mat, AnswerSheet), UploadError>>),
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
        let base64_list: Vec<Result<(String, Mat, AnswerSheet), UploadError>> = paths
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
    let mut dst = new_mat_copy!(src);
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
) -> Result<(String, Mat, AnswerSheet), UploadError> {
    let mat = read_from_path(path)?;
    let mut splitted = prepare_answer_sheet(mat)?;

    let original = mem::take(&mut splitted.original);
    let base64 = mat_to_base64_png(&original).map_err(UploadError::from)?;
    let answer_sheet = AnswerSheet::try_convert(splitted, ocr)?;
    Ok((base64, original, answer_sheet))
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

fn crop_to_markers(mat: Mat) -> Result<Mat, SheetError> {
    let grayscaled = {
        let mut gray = new_mat_copy!(mat);
        imgproc::cvt_color_def(&mat, &mut gray, imgproc::COLOR_BGR2GRAY)?;
        gray
    };
    let blurred = {
        let mut blur = new_mat_copy!(grayscaled);
        imgproc::gaussian_blur_def(&grayscaled, &mut blur, (5, 5).into(), 0.0)?;
        blur
    };
    let thresholded = {
        let mut thresh = new_mat_copy!(blurred);
        imgproc::adaptive_threshold(
            &blurred,
            &mut thresh,
            255.0,
            imgproc::ADAPTIVE_THRESH_GAUSSIAN_C,
            imgproc::THRESH_BINARY_INV,
            11,
            2.0,
        )?;
        thresh
    };

    let contours = {
        let mut vec: Vector<Vector<Point>> = vec![].into();
        imgproc::find_contours_def(
            &thresholded,
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

    Ok(resize_relative_img(&cropped, 0.3333)?)
}

fn prepare_answer_sheet(mat: Mat) -> Result<SplittedSheet, SheetError> {
    let cropped = crop_to_markers(mat)?;
    let splitted = split_into_areas(cropped)?;
    Ok(splitted)
}

fn split_into_areas(sheet: Mat) -> Result<SplittedSheet, SheetError> {
    fn roi_range_frac(
        mat: &Mat,
        x: RangeInclusive<f64>,
        y: RangeInclusive<f64>,
    ) -> opencv::Result<Mat> {
        let (range_x, range_y) = (x.end() - x.start(), y.end() - y.start());
        let (width, height) = (mat.cols(), mat.rows());
        let (start_x, start_y) = (width as f64 * x.start(), height as f64 * y.start());
        let (range_width, range_height) = (width as f64 * range_x, height as f64 * range_y);
        Ok(mat
            .roi(Rect_ {
                x: start_x as i32,
                y: start_y as i32,
                width: range_width as i32,
                height: range_height as i32,
            })?
            .clone_pointee())
    }

    const START_PERCENT_X: f64 = 0.18525022;
    const START_PERCENT_Y: f64 = 0.010113780;
    const WIDTH_PERCENT: f64 = 0.19841967;
    const HEIGHT_PERCENT: f64 = 0.094816688;
    const GAP_X_PERCENT: f64 = 0.0079016681;
    const GAP_Y_PERCENT: f64 = 0.015170670;

    let subject_name = roi_range_frac(&sheet, 0.01317..=0.1765, 0.1479..=0.1656)?;
    let student_name = roi_range_frac(&sheet, 0.0342..=0.1773, 0.1113..=0.1340)?;
    let exam_room = roi_range_frac(&sheet, 0.032484636..=0.088674276, 0.206068268..=0.230088496)?;
    let exam_seat = roi_range_frac(&sheet, 0.134328358..=0.175592625, 0.206068268..=0.230088496)?;
    let subject_id = roi_range_frac(&sheet, 0.0..=0.040386304, 0.271807838..=0.517067004)?;
    let student_id = roi_range_frac(&sheet, 0.049165935..=0.177348551, 0.273072061..=0.515802781)?;
    let questions = {
        let ranges = (0..4).flat_map(|x| (0..9).map(move |y| (x, y)));
        ranges
            .map(|(x, y)| {
                let min_x = START_PERCENT_X + x as f64 * (GAP_X_PERCENT + WIDTH_PERCENT);
                let max_x = f64::min(min_x + WIDTH_PERCENT, 1.0);
                let min_y = START_PERCENT_Y + y as f64 * (GAP_Y_PERCENT + HEIGHT_PERCENT);
                let max_y = f64::min(min_y + HEIGHT_PERCENT, 1.0);
                Ok(roi_range_frac(&sheet, min_x..=max_x, min_y..=max_y)?)
            })
            .collect::<opencv::Result<Vec<Mat>>>()?
    };

    Ok(SplittedSheet {
        original: sheet,
        student_name,
        subject_name,
        exam_room,
        exam_seat,
        subject_id,
        student_id,
        questions,
    })
}

fn extract_answers(answer_mats: Vec<Mat>) -> Result<[QuestionGroup; 36], SheetError> {
    todo!();
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
                x.clamp(0, answer_mats.cols() - ANSWER_WIDTH),
                y.clamp(0, answer_mats.rows() - ANSWER_HEIGHT),
            );
            let rect = Rect_ {
                x,
                y,
                width: ANSWER_WIDTH,
                height: ANSWER_HEIGHT,
            };
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
                            let bubble_filled: u16 = answer_mats
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
                        .filter_map(|(idx, f)| if f < 0.39 { Some(idx) } else { None })
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
    let the_height_from_above_to_bubble = 47;
    let digit_height = 16;
    let digit_width = 15;
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

        let mut max_num = u32::MIN;
        let mut selected_digit = 0;

        for j in 0..10usize {
            let y = (j as i32) * digit_height + the_height_from_above_to_bubble;
            let digit_roi = roi.roi(Rect_ {
                x: 0,
                y,
                width: digit_width,
                height: digit_height,
            })?;

            let mut bin = Mat::default();
            let _ = opencv::imgproc::threshold(
                &digit_roi,
                &mut bin,
                0.0,
                255.0,
                opencv::imgproc::THRESH_BINARY_INV,
            );
            let sum: u32 = opencv::core::count_non_zero(&bin)? as u32;
            if temp {
                if i > 0 {
                    v[i - 1][j] = sum as i32; //Skip first Index NaKub
                }
            } else {
                v[i][j] = sum as i32;
            }

            if sum > max_num {
                max_num = sum;
                selected_digit = j;
            }
        }
        digits.push_str(&selected_digit.to_string());
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
        let subject_id = extract_digits_for_sub_stu(&subject_id_mat, 2, false)?;
        let student_id = extract_digits_for_sub_stu(&student_id_mat, 9, true)?;
        let answers = extract_answers(questions)?;

        let (mut student_name, mut subject_name, mut exam_room, mut exam_seat) =
            (None, None, None, None);
        if let Some(ocr) = ocr {
            let (written_subject_id, written_student_id) =
                extract_subject_student_from_written_field(subject_id_mat, student_id_mat, ocr)?;
            if subject_id != written_subject_id || student_id != written_student_id {
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
                let verdict = self.graded_questions[(x_idx * 9 + y_idx) as usize]
                    .0
                    .verdict();
                let question_color: Option<opencv::core::Scalar> = match verdict {
                    CheckedAnswer::Correct => Some((43, 160, 64).into()),
                    CheckedAnswer::Incorrect => Some((57, 15, 210).into()),
                    CheckedAnswer::Missing => Some((29, 142, 223).into()),
                    CheckedAnswer::NotCounted => None,
                };
                if let Some(question_color) = question_color {
                    imgproc::rectangle(
                        &mut sheet_transparent,
                        Rect_ {
                            x,
                            y,
                            width: 24,
                            height: ANSWER_HEIGHT,
                        },
                        question_color,
                        FILLED,
                        LINE_8,
                        0,
                    )?;
                }
                for row_idx in 0..5usize {
                    let result_here = self.graded_questions[(x_idx * 9 + y_idx) as usize]
                        .0
                        .at(row_idx);
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

fn image_to_string(mat: &Mat, ocr: &OcrEngine) -> Result<String, SheetError> {
    let bytes = mat.data_bytes()?;
    let img_src = ImageSource::from_bytes(bytes, (mat.cols() as u32, mat.rows() as u32))
        .map_err(|e| anyhow::anyhow!(e))?;
    let ocr_input = ocr.prepare_input(img_src)?;
    let text = ocr.get_text(&ocr_input)?;
    let rem_nl = text.lines().next().unwrap_or("").to_string();

    Ok(rem_nl)
}

fn extract_user_information(
    name: Mat,
    subject_name: Mat,
    exam_room: Mat,
    exam_seat: Mat,
    ocr: &OcrEngine,
) -> Result<(String, String, String, String), SheetError> {
    #[cfg(debug_assertions)]
    {
        let temp_dir = "temp";
        _ = std::fs::create_dir_all(temp_dir);

        debug!("Working directory: {:?}", std::env::current_dir());
        safe_imwrite("temp/debug_name.png", &name)?;
        safe_imwrite("temp/debug_subject.png", &subject_name)?;
        safe_imwrite("temp/debug_exam_room.png", &exam_room)?;
        safe_imwrite("temp/debug_seat.png", &exam_seat)?;
    }

    let name_string = image_to_string(&name, ocr)?;
    let subject_string = image_to_string(&subject_name, ocr)?;
    let exam_room_string = image_to_string(&exam_room, ocr)?;
    let seat_string = image_to_string(&exam_seat, ocr)?;

    Ok((name_string, subject_string, exam_room_string, seat_string))
}

fn safe_imwrite<P: AsRef<Path>>(path: P, mat: &Mat) -> Result<bool, opencv::Error> {
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

fn crop_subject_stuent(mat: &Mat) -> Result<(Mat, Mat), SheetError> {
    let subject = mat
        .roi(Rect_ {
            x: 40,
            y: 245,
            width: 43,
            height: 19,
        })?
        .clone_pointee();
    let student = mat
        .roi(Rect_ {
            x: 112,
            y: 245,
            width: 120,
            height: 18,
        })?
        .clone_pointee();
    Ok((subject, student))
}

fn extract_subject_student_from_written_field(
    subject_id_mat: Mat,
    student_id_mat: Mat,
    ocr: &OcrEngine,
) -> Result<(String, String), SheetError> {
    let rsub = image_to_string(&subject_id_mat, ocr)?;
    let rstu = image_to_string(&student_id_mat, ocr)?;

    let subject = clean_text(&rsub);
    let student = clean_text(&rstu);

    safe_imwrite("temp/debug_subject_f.png", &subject_id_mat)?;
    safe_imwrite("temp/debug_student_f.png", &student_id_mat)?;

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

        let (base64_string, mat, _answer_sheet) = result.unwrap();
        assert!(base64_string.starts_with("data:image/png;base64,"));
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
        let mat_no_markers = new_mat_copy!(mat_markers);
        let cropped_ok = crop_to_markers(mat_markers);
        assert!(cropped_ok.is_ok());
        let cropped_not_ok = crop_to_markers(mat_no_markers);
        assert!(cropped_not_ok.is_err());
        assert!(matches!(
            cropped_not_ok.unwrap_err(),
            SheetError::MissingMarkers
        ));

        for image in test_images() {
            println!("testing image {}", image);
            let mat = read_from_path(image).unwrap();
            let cropped = crop_to_markers(mat);
            assert!(cropped.is_ok());
        }
    }

    #[test]
    fn test_split_into_areas() {
        for image in test_images() {
            println!("testing image {}", image);
            let mat = read_from_path(image).unwrap();
            let cropped = crop_to_markers(mat).unwrap();
            let splitted = split_into_areas(cropped);
            assert!(splitted.is_ok());
        }
    }

    #[test]
    fn check_extracted_ids_from_real_image() {
        let path = test_key_image();
        let mat = read_from_path(path).expect("Failed to read image");
        let SplittedSheet {
            subject_id,
            student_id,
            ..
        } = prepare_answer_sheet(mat).expect("Fixing sheet failed");

        let subject_id = extract_digits_for_sub_stu(&subject_id, 2, false)
            .expect("Extracting subject ID failed");
        let student_id =
            extract_digits_for_sub_stu(&student_id, 9, true).expect("Extracting student ID failed");

        assert_eq!(subject_id, "10", "Subject ID does not match expected value");
        assert_eq!(
            student_id, "65010001",
            "Student ID does not match expected value"
        );
    }

    #[test]
    #[cfg(feature = "ocr-tests")]
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
            let (subject_id, student_id) =
                extract_subject_student_from_written_field(subject_id, student_id, ocr)?;

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
    #[cfg(feature = "ocr-tests")]
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

            let (name, subject, exam_room, seat) =
                extract_user_information(student_name, subject_name, exam_room, exam_seat, ocr)?;
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
                    name, "Sophie-Louise Greene",
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
