use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Mutex, RwLock},
};
use tauri::{ipc::Channel, Emitter, Manager, Runtime};

use opencv::core::Mat;

use crate::{
    errors::{SheetError, UploadError},
    scoring::{AnswerSheetResult, CheckedAnswer},
};

pub type StateMutex = Mutex<AppState>;

#[macro_export]
macro_rules! signal {
    ($channel: ident, $message: expr) => {
        if let Err(e) = $channel.send($message) {
            println!("Channel emission failed: {e}");
        }
    };
}

#[derive(Default)]
pub enum AppState {
    #[default]
    Init,
    WithKey {
        key_image: Mat,
        key: AnswerKeySheet,
        subject_code: String,
    },
    Scored {
        key_image: Mat,
        key: AnswerKeySheet,
        subject_code: String,
        answer_sheets: HashMap<String, (Mat, AnswerSheet, AnswerSheetResult)>,
    },
}

impl AppState {
    pub fn upload_key<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: Channel<KeyUpload>,
        base64_image: String,
        image: Mat,
        subject_code: String,
        key: AnswerKeySheet,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        match *state {
            AppState::Init | AppState::WithKey { .. } => {
                *state = AppState::WithKey {
                    key_image: image,
                    key,
                    subject_code,
                };
                signal!(
                    channel,
                    KeyUpload::Done {
                        base64: base64_image
                    }
                );
            }
            _ => (),
        }
    }
    pub fn clear_key<R: Runtime, A: Emitter<R> + Manager<R>>(app: &A, channel: Channel<KeyUpload>) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        if let AppState::WithKey { .. } = *state {
            *state = AppState::Init;
            signal!(channel, KeyUpload::Clear);
        }
    }
    pub fn upload_answer_sheets<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: Channel<AnswerUpload>,
        result: Vec<Result<(String, Mat, AnswerSheet), UploadError>>,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        match &*state {
            AppState::WithKey {
                key_image,
                key,
                subject_code,
            }
            | AppState::Scored {
                key_image,
                key,
                subject_code,
                ..
            } => {
                let key = RwLock::new(key.clone());
                let scored: Vec<
                    Result<(String, Mat, AnswerSheet, AnswerSheetResult), UploadError>,
                > = result
                    .into_par_iter()
                    .map(|r| {
                        r.map(|(s, m, a)| {
                            let score = a.score(&key.read().expect("should not panic"));
                            (s, m, a, score)
                        })
                    })
                    .collect();
                let to_send: Vec<AnswerScoreResult> = scored
                    .par_iter()
                    .map(|r| match r {
                        Ok((
                            base64,
                            _,
                            _,
                            AnswerSheetResult {
                                correct,
                                incorrect,
                                not_answered,
                                ..
                            },
                        )) => AnswerScoreResult::Ok {
                            base64: base64.clone(),
                            correct: *correct,
                            incorrect: *incorrect,
                            not_answered: *not_answered,
                        },
                        Err(e) => AnswerScoreResult::Error {
                            error: format!("{e}"),
                        },
                    })
                    .collect();
                let answer_sheets = scored
                    .into_par_iter()
                    .filter_map(|r| {
                        if let Ok((_, m, a, ca)) = r {
                            Some((a.student_id.clone(), (m, a, ca)))
                        } else {
                            None
                        }
                    })
                    .collect();
                *state = AppState::Scored {
                    key_image: key_image.clone(),
                    key: key.into_inner().expect("should not panic"),
                    subject_code: subject_code.clone(),
                    answer_sheets,
                };
                signal!(channel, AnswerUpload::Done { uploaded: to_send });
            }
            _ => (),
        }
    }
    pub fn clear_answer_sheets<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: Channel<AnswerUpload>,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        if let AppState::Scored {
            key,
            key_image,
            subject_code,
            ..
        } = &*state
        {
            *state = AppState::WithKey {
                key_image: key_image.clone(),
                key: key.clone(),
                subject_code: subject_code.clone(),
            };
            signal!(channel, AnswerUpload::Clear);
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnswerSheet {
    pub subject_code: String,
    pub student_id: String,
    pub answers: [QuestionGroup; 36],
}

#[derive(Debug, Clone)]
pub struct AnswerKeySheet {
    pub subject_code: String,
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

#[allow(non_snake_case)]
#[derive(Debug, Clone)]
pub struct QuestionGroup {
    pub A: Option<Answer>,
    pub B: Option<Answer>,
    pub C: Option<Answer>,
    pub D: Option<Answer>,
    pub E: Option<Answer>,
}

impl TryFrom<Vec<Option<Answer>>> for QuestionGroup {
    type Error = SheetError;
    fn try_from(value: Vec<Option<Answer>>) -> Result<Self, Self::Error> {
        let mut iter = value.into_iter();
        Ok(Self {
            A: iter.next().ok_or(SheetError::TooLittleAnswers)?,
            B: iter.next().ok_or(SheetError::TooLittleAnswers)?,
            C: iter.next().ok_or(SheetError::TooLittleAnswers)?,
            D: iter.next().ok_or(SheetError::TooLittleAnswers)?,
            E: iter.next().ok_or(SheetError::TooLittleAnswers)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Answer {
    pub num_type: Option<NumberType>,
    pub number: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberType {
    Plus,
    Minus,
    PlusOrMinus,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum KeyUpload {
    Cancelled,
    Clear,
    Done { base64: String },
    Error { error: String },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum AnswerUpload {
    Cancelled,
    Clear,
    Processing {
        total: usize,
        started: usize,
        finished: usize,
    },
    AlmostDone,
    Done {
        uploaded: Vec<AnswerScoreResult>,
    },
    Error {
        error: String,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "result",
    content = "data"
)]
pub enum AnswerScoreResult {
    Ok {
        base64: String,
        correct: u32,
        incorrect: u32,
        not_answered: u32,
    },
    Error {
        error: String,
    },
}

#[cfg(test)]
mod unit_tests {
    use crate::image::upload_key_image_impl;
    use crate::image::upload_sheet_images_impl;
    use std::sync::Arc;
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
        let msg_history: Arc<Mutex<Vec<KeyUpload>>> = Arc::new(Mutex::new(vec![]));
        let msg_hist_ref = Arc::clone(&msg_history);
        let channel: Channel<KeyUpload> = Channel::new(move |msg| {
            let mut vec = msg_hist_ref.lock().unwrap();
            let msg: KeyUpload = msg.deserialize().unwrap();
            vec.push(msg);
            Ok(())
        });
        upload_key_image_impl(&app, Some(test_key_image()), channel);

        assert_state!(app, AppState::WithKey { .. });
        let msg_history = Arc::into_inner(msg_history).unwrap().into_inner().unwrap();
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
            let AppState::WithKey { key_image, key } = &*state else {
                unreachable!()
            };
            key_image.clone()
        };

        upload_key_image_impl(&app, Some(path2));

        let mutex = app.state::<StateMutex>();
        let state = mutex.lock().unwrap();
        if let AppState::WithKey { key_image, key } = &*state {
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

        assert_state!(app, AppState::Scored { .. });
    }
    #[test]
    fn test_app_change_sheets_upload() {
        let app = mock_app_with_state(AppState::Init);
        upload_key_image_impl(&app, Some(test_key_image()));
        upload_sheet_images_impl(&app, Some(test_images()));

        let current_count = {
            let mutex = app.state::<StateMutex>();
            let state = mutex.lock().expect("poisoned");
            let AppState::Scored { sheet_images, .. } = &*state else {
                unreachable!()
            };
            sheet_images.len()
        };

        upload_sheet_images_impl(&app, Some(vec![test_images()[0].clone()]));

        let mutex = app.state::<StateMutex>();
        let state = mutex.lock().unwrap();
        if let AppState::Scored { sheet_images, .. } = &*state {
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

        assert_state!(app, AppState::Scored { .. });

        AppState::clear_answer_sheets(&app);

        assert_state!(app, AppState::WithKey { .. });
    }

    #[test]
    fn test_clear_key_on_with_key_and_sheets_does_nothing() {
        let app = mock_app_with_state(AppState::Init);
        upload_key_image_impl(&app, Some(test_key_image()));
        upload_sheet_images_impl(&app, Some(test_images()));

        assert_state!(app, AppState::Scored { .. });

        AppState::clear_key(&app);

        // Should still be in WithKeyAndSheets
        assert_state!(app, AppState::Scored { .. });
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
