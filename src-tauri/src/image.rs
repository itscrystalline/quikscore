use crate::errors::{SheetError, UploadError};
use crate::signal;
use crate::state::{AppState, SignalKeys, StateMutex};
use base64::Engine;
use opencv::core::{Mat, Moments, Point, Rect_, Size, Vector};
use opencv::imgcodecs::{imencode, imread, ImreadModes};
use opencv::imgproc::{
    ADAPTIVE_THRESH_GAUSSIAN_C, CHAIN_APPROX_SIMPLE, FILLED, LINE_8, RETR_EXTERNAL,
    THRESH_BINARY_INV,
};
use opencv::prelude::*;
use opencv::{highgui, imgproc, prelude::*};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::FilePath;

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

pub fn upload_sheet_images_impl(app: AppHandle, paths: Vec<FilePath>) {
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

fn resize_img(src: &Mat) -> opencv::Result<Mat> {
    let mut dst = new_mat_copy!(src);
    let new_size = Size::new(src.cols() / 3, src.rows() / 3);

    imgproc::resize(src, &mut dst, new_size, 0.0, 0.0, imgproc::INTER_LINEAR)?;
    Ok(dst)
}

fn show_img(mat: &Mat, window_name: &str) -> opencv::Result<()> {
    highgui::imshow(window_name, mat)?;
    highgui::wait_key(0)?;
    Ok(())
}

fn handle_upload(path: FilePath) -> Result<(String, Mat), UploadError> {
    let mat = read_from_path(path)?;
    let resized = resize_img(&mat).map_err(UploadError::from)?;
    let aligned = fix_answer_sheet(resized)?;
    //testing
    #[cfg(not(test))]
    let _ = show_img(&aligned, "resized & aligned image");
    let base64 = mat_to_base64_png(&aligned).map_err(UploadError::from)?;
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

fn preprocess_sheet(mat: Mat) -> Result<Mat, SheetError> {
    // blur
    let mut mat_blur = new_mat_copy!(mat);
    imgproc::gaussian_blur_def(&mat, &mut mat_blur, (3, 3).into(), 0.0)?;
    // thresholding
    let mut mat_thresh = new_mat_copy!(mat);
    imgproc::adaptive_threshold(
        &mat_blur,
        &mut mat_thresh,
        255.0,
        ADAPTIVE_THRESH_GAUSSIAN_C,
        THRESH_BINARY_INV,
        11,
        2.0,
    )?;
    Ok(mat_thresh)
}

fn find_markers(mat_ref: &Mat, mat_debug: &mut Mat) -> Result<Markers, SheetError> {
    // contours
    let contours: Vector<Vector<Point>> = {
        let mut contours: Vector<Vector<Point>> = vec![].into();
        imgproc::find_contours_def(&mat_ref, &mut contours, RETR_EXTERNAL, CHAIN_APPROX_SIMPLE)?;
        contours
    };

    // imgproc::draw_contours_def(mat_debug, &contours, -1, (0, 255, 0).into())?;

    let corner_markers: Vec<(i32, i32, usize)> = contours
        .to_vec()
        .into_iter()
        .filter_map(|contour| {
            let peri = imgproc::arc_length(&contour, true).ok()?;
            let approx = {
                let mut approx: Vector<Point> = vec![].into();
                imgproc::approx_poly_dp(&contour, &mut approx, 0.04 * peri, true).ok()?;
                approx
            };
            let area = imgproc::contour_area_def(&contour).ok()?;
            if !(50.0..).contains(&area) {
                return None;
            }

            match approx.len() {
                3 => {
                    let Moments { m00, m10, m01, .. } = imgproc::moments_def(&contour).ok()?;
                    if m00 == 0.0 {
                        return None;
                    }
                    let mut tmp: Vector<Vector<Point>> = Vector::new();
                    tmp.push(contour.clone());
                    _ = imgproc::draw_contours_def(mat_debug, &tmp, -1, (0, 255, 0).into());
                    Some(((m10 / m00) as i32, (m01 / m00) as i32, 3))
                }
                4 => {
                    let Rect_ { width, height, .. } = imgproc::bounding_rect(&contour).ok()?;
                    let aspect_ratio = width as f32 / height as f32;
                    if !(1.5..1.7).contains(&aspect_ratio) {
                        return None;
                    }
                    let Moments { m00, m10, m01, .. } = imgproc::moments_def(&contour).ok()?;
                    if m00 == 0.0 {
                        return None;
                    }
                    let mut tmp: Vector<Vector<Point>> = Vector::new();
                    tmp.push(contour.clone());
                    _ = imgproc::draw_contours_def(mat_debug, &tmp, -1, (0, 255, 0).into());
                    Some(((m10 / m00) as i32, (m01 / m00) as i32, 4))
                }
                _ => None,
            }
        })
        .inspect(|&(x, y, corners)| {
            println!("found marker with {corners} corners at x: {x}, y: {y}")
            // if corners == 3 {
            //     _ = imgproc::circle(
            //         mat_debug,
            //         Point { x, y },
            //         2,
            //         (192, 255, 0).into(),
            //         FILLED,
            //         LINE_8,
            //         0,
            //     );
            // } else {
            //     _ = imgproc::circle(
            //         mat_debug,
            //         Point { x, y },
            //         2,
            //         (255, 192, 0).into(),
            //         FILLED,
            //         LINE_8,
            //         0,
            //     );
            // }
        })
        .collect();

    let (mut tri_markers, mut rect_markers): (Vec<_>, Vec<_>) = corner_markers
        .into_iter()
        .partition(|&(_, _, corners)| corners == 3);
    rect_markers.sort_by(|&(_, ay, _), &(_, by, _)| ay.cmp(&by));
    tri_markers.sort_by(|&(_, ay, _), &(_, by, _)| ay.cmp(&by));

    let top_center = rect_markers
        .first()
        .ok_or(SheetError::MarkerNotFound)?
        .to_owned();
    let bottom_center = rect_markers
        .last()
        .ok_or(SheetError::MarkerNotFound)?
        .to_owned();

    let bottom_left = tri_markers
        .last()
        .ok_or(SheetError::MarkerNotFound)?
        .to_owned();

    tri_markers.sort_by(|&(ax, _, _), &(bx, _, _)| ax.cmp(&bx));
    let mut tri_markers = tri_markers.into_iter();

    let top_left = tri_markers
        .next()
        .ok_or(SheetError::MarkerNotFound)?
        .to_owned();
    let top_right = tri_markers
        .next()
        .ok_or(SheetError::MarkerNotFound)?
        .to_owned();

    let markers = Markers {
        top_left: (top_left.0, top_left.1).into(),
        top_right: (top_right.0, top_right.1).into(),
        bottom_left: (bottom_left.0, bottom_left.1).into(),
        top_center: (top_center.0, top_center.1).into(),
        bottom_center: (bottom_center.0, bottom_center.1).into(),
    };

    markers.draw_markers_debug(mat_debug);

    Ok(markers)
}

fn fix_answer_sheet(mat: Mat) -> Result<Mat, SheetError> {
    let mut debug_image = new_mat_copy!(mat);
    imgproc::cvt_color_def(&mat, &mut debug_image, imgproc::COLOR_GRAY2RGB)?;

    let preprocessed = preprocess_sheet(mat)?;
    let contours = find_markers(&preprocessed, &mut debug_image);

    Ok(debug_image)
}

#[derive(Debug)]
struct Markers {
    top_left: Point,
    top_right: Point,
    bottom_left: Point,
    top_center: Point,
    bottom_center: Point,
}
impl Markers {
    fn infer_bottom_right(&self) -> Point {
        let dx = self.bottom_left.x - self.top_left.x;
        let dy = self.bottom_left.y - self.top_left.y;
        opencv::core::Point_ {
            x: self.top_right.x + dx,
            y: self.top_right.y + dy,
        }
    }

    fn draw_markers_debug(&self, mat_debug: &mut Mat) {
        for point in [
            self.top_left,
            self.top_right,
            self.top_center,
            self.bottom_left,
            self.bottom_center,
        ] {
            _ = imgproc::circle(mat_debug, point, 3, (0, 0, 255).into(), FILLED, LINE_8, 0);
        }
    }
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
    #[test]
    fn test_resize_img() {
        let width = 300;
        let height = 300;
        let mat =
            Mat::new_rows_cols_with_default(height, width, core::CV_8UC1, core::Scalar::all(128.0))
                .unwrap();

        let resized = resize_img(&mat);
        assert!(resized.is_ok());

        let resized = resized.unwrap();
        assert!(!resized.empty());

        assert_eq!(resized.cols(), width / 3);
        assert_eq!(resized.rows(), height / 3);
    }
}
