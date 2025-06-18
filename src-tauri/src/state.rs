use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use opencv::core::Mat;

pub type StateMutex = Mutex<AppState>;

#[macro_export]
macro_rules! signal {
    ($app: ident, $message_key: expr, $message: expr) => {
        if let Err(e) = $app.emit($message_key.into(), $message) {
            println!("Signal emission failed: {e}");
        }
    };
}

#[derive(Default)]
pub enum AppState {
    #[default]
    Init,
    WithKey {
        key_image: Mat,
        // key: AnswerKeySheet,
    },
    WithKeyAndSheets {
        key_image: Mat,
        // key: AnswerKeySheet,
        sheet_images: Vec<Mat>,
        // _answer_sheets: Vec<AnswerSheet>,
    },
}

impl AppState {
    pub fn upload_key<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        base64_image: String,
        image: Mat,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        match *state {
            AppState::Init | AppState::WithKey { .. } => {
                *state = AppState::WithKey {
                    key_image: image,
                    // key: answer.into(),
                };
                signal!(app, SignalKeys::KeyImage, base64_image);
                signal!(app, SignalKeys::KeyStatus, "");
            }
            _ => (),
        }
    }
    pub fn clear_key<R: Runtime, A: Emitter<R> + Manager<R>>(app: &A) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        if let AppState::WithKey { .. } = *state {
            *state = AppState::Init;
            signal!(app, SignalKeys::KeyImage, "");
            signal!(app, SignalKeys::KeyStatus, "");
        }
    }
    pub fn upload_answer_sheets<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        base64_images: Vec<String>,
        images: Vec<Mat>,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        match &*state {
            AppState::WithKey {
                key_image,
                // ref key,
            }
            | AppState::WithKeyAndSheets {
                key_image,
                // ref key,
                ..
            } => {
                *state = AppState::WithKeyAndSheets {
                    key_image: key_image.clone(),
                    // key: key.clone(),
                    sheet_images: images,
                    // _answer_sheets: vec_answers,
                };
                signal!(app, SignalKeys::SheetImages, base64_images);
                signal!(app, SignalKeys::SheetStatus, "");
            }
            _ => (),
        }
    }
    pub fn clear_answer_sheets<R: Runtime, A: Emitter<R> + Manager<R>>(app: &A) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        if let AppState::WithKeyAndSheets {
            /*key,*/ key_image,
            ..
        } = &*state
        {
            *state = AppState::WithKey {
                key_image: key_image.clone(),
                // key,
            };
            signal!(app, SignalKeys::SheetImages, Vec::<String>::new());
            signal!(app, SignalKeys::SheetStatus, "");
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AnswerSheet {
    pub subject_code: u16,
    pub student_id: u32,
    pub answers: [QuestionGroup; 36],
}

#[derive(Debug, Clone, Copy)]
pub struct AnswerKeySheet {
    pub subject_code: u16,
    pub answers: [QuestionGroup; 36],
}
impl From<AnswerSheet> for AnswerKeySheet {
    fn from(value: AnswerSheet) -> Self {
        Self {
            subject_code: value.subject_code,
            answers: value.answers,
        }
    }
}

impl From<(Mat, Mat, Mat)> for AnswerSheet {
    fn from(value: (Mat, Mat, Mat)) -> Self {
        let (subject_code_mat, student_id_mat, answers_mat) = value;
        todo!()
    }
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Copy)]
pub struct QuestionGroup {
    pub A: Option<Answer>,
    pub B: Option<Answer>,
    pub C: Option<Answer>,
    pub D: Option<Answer>,
    pub E: Option<Answer>,
}

#[derive(Debug, Clone, Copy)]
pub struct Answer {
    pub num_type: Option<NumberType>,
    pub number: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum NumberType {
    Plus,
    Minus,
    PlusOrMinus,
}

pub enum SignalKeys {
    KeyStatus,
    KeyImage,
    SheetStatus,
    SheetImages,
}
impl From<SignalKeys> for &str {
    fn from(value: SignalKeys) -> Self {
        match value {
            SignalKeys::KeyStatus => "key-status",
            SignalKeys::KeyImage => "key-image",
            SignalKeys::SheetStatus => "sheet-status",
            SignalKeys::SheetImages => "sheet-images",
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use crate::image::upload_key_image_impl;
    use crate::image::upload_sheet_images_impl;
    use std::{path::PathBuf, sync::Mutex};

    use crate::state::StateMutex;

    use super::*;
    use opencv::core::{self, CMP_NE};
    use opencv::prelude::*;
    use tauri::{test::MockRuntime, App, Manager};
    use tauri_plugin_fs::FilePath;

    fn mock_app_with_state(state: AppState) -> App<MockRuntime> {
        let app = tauri::test::mock_app();
        app.manage(Mutex::new(state));
        app
    }

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

    fn compare_mats(a: &Mat, b: &Mat) -> bool {
        if a.size().unwrap() != b.size().unwrap() || a.typ() != b.typ() {
            return false;
        }

        // Compare pixel-by-pixel
        let mut diff = Mat::default();
        core::compare(a, b, &mut diff, CMP_NE).unwrap();
        let nz = core::count_non_zero(&diff).unwrap();
        nz == 0
    }

    macro_rules! assert_state {
        ($app: ident, $pattern:pat $(if $guard:expr)? $(,)?) => {{
            let mutex = $app.state::<StateMutex>();
            let state = mutex.lock().unwrap();
            assert!(matches!(*state, $pattern $(if $guard)?));
        }};
    }

    #[test]
    fn test_app_key_upload() {
        let app = mock_app_with_state(AppState::Init);
        upload_key_image_impl(&app, Some(test_key_image()));

        assert_state!(app, AppState::WithKey { .. });
    }
    #[test]
    fn test_app_change_key_upload() {
        let path = test_key_image();
        let path2 = test_images()[1].clone();
        let app = mock_app_with_state(AppState::Init);

        upload_key_image_impl(&app, Some(path));

        let current_mat = {
            let mutex = app.state::<StateMutex>();
            let state = mutex.lock().expect("poisoned");
            let AppState::WithKey { key_image } = &*state else {
                unreachable!()
            };
            key_image.clone()
        };

        upload_key_image_impl(&app, Some(path2));

        let mutex = app.state::<StateMutex>();
        let state = mutex.lock().unwrap();
        if let AppState::WithKey { key_image } = &*state {
            assert!(!compare_mats(key_image, &current_mat));
        } else {
            unreachable!()
        }
    }
    #[test]
    fn test_app_key_canceled_upload() {
        let app = mock_app_with_state(AppState::Init);
        upload_key_image_impl(&app, None);

        assert_state!(app, AppState::Init);
    }
    #[test]
    fn test_app_key_invalid_upload() {
        let app = mock_app_with_state(AppState::Init);
        upload_key_image_impl(&app, Some(not_image()));

        assert_state!(app, AppState::Init);
    }
    #[test]
    fn test_app_key_clear() {
        let app = mock_app_with_state(AppState::Init);
        upload_key_image_impl(&app, Some(test_key_image()));

        assert_state!(app, AppState::WithKey { .. });

        AppState::clear_key(&app);

        assert_state!(app, AppState::Init);
    }

    #[test]
    fn test_app_sheets_upload() {
        let app = mock_app_with_state(AppState::Init);
        upload_key_image_impl(&app, Some(test_key_image()));
        upload_sheet_images_impl(&app, Some(test_images()));

        assert_state!(app, AppState::WithKeyAndSheets { .. });
    }
    #[test]
    fn test_app_change_sheets_upload() {
        let app = mock_app_with_state(AppState::Init);
        upload_key_image_impl(&app, Some(test_key_image()));
        upload_sheet_images_impl(&app, Some(test_images()));

        let current_count = {
            let mutex = app.state::<StateMutex>();
            let state = mutex.lock().expect("poisoned");
            let AppState::WithKeyAndSheets { sheet_images, .. } = &*state else {
                unreachable!()
            };
            sheet_images.len()
        };

        upload_sheet_images_impl(&app, Some(vec![test_images()[0].clone()]));

        let mutex = app.state::<StateMutex>();
        let state = mutex.lock().unwrap();
        if let AppState::WithKeyAndSheets { sheet_images, .. } = &*state {
            assert_ne!(current_count, sheet_images.len());
        } else {
            unreachable!()
        }
    }
    #[test]
    fn test_app_sheets_canceled_upload() {
        let app = mock_app_with_state(AppState::Init);
        upload_key_image_impl(&app, Some(test_key_image()));
        upload_sheet_images_impl(&app, None);

        assert_state!(app, AppState::WithKey { .. });
    }
    #[test]
    fn test_app_sheets_invalid_upload() {
        let app = mock_app_with_state(AppState::Init);
        upload_key_image_impl(&app, Some(test_key_image()));
        upload_sheet_images_impl(&app, Some(vec![not_image()]));

        assert_state!(app, AppState::WithKey { .. });
    }
    #[test]
    fn test_app_sheets_clear() {
        let app = mock_app_with_state(AppState::Init);
        upload_key_image_impl(&app, Some(test_key_image()));
        upload_sheet_images_impl(&app, Some(test_images()));

        assert_state!(app, AppState::WithKeyAndSheets { .. });

        AppState::clear_answer_sheets(&app);

        assert_state!(app, AppState::WithKey { .. });
    }

    #[test]
    fn test_clear_key_on_with_key_and_sheets_does_nothing() {
        let app = mock_app_with_state(AppState::Init);
        upload_key_image_impl(&app, Some(test_key_image()));
        upload_sheet_images_impl(&app, Some(test_images()));

        assert_state!(app, AppState::WithKeyAndSheets { .. });

        AppState::clear_key(&app);

        // Should still be in WithKeyAndSheets
        assert_state!(app, AppState::WithKeyAndSheets { .. });
    }
    #[test]
    fn test_clear_answer_sheets_on_init_does_nothing() {
        let app = mock_app_with_state(AppState::Init);
        AppState::clear_answer_sheets(&app);
        assert_state!(app, AppState::Init);
    }
    #[test]
    fn test_clear_answer_sheets_on_with_key_does_nothing() {
        let app = mock_app_with_state(AppState::Init);
        upload_key_image_impl(&app, Some(test_key_image()));

        assert_state!(app, AppState::WithKey { .. });

        AppState::clear_answer_sheets(&app);

        assert_state!(app, AppState::WithKey { .. });
    }
    #[test]
    fn test_upload_sheets_without_key_does_nothing() {
        let app = mock_app_with_state(AppState::Init);
        upload_sheet_images_impl(&app, Some(test_images()));

        // Should remain in Init because upload_sheets does nothing without a key
        assert_state!(app, AppState::Init);
    }
}
