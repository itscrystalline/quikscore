use crate::err_log;
use crate::ocr::OcrEngine;
use log::{error, info};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use std::{
    array,
    collections::HashMap,
    fmt::Display,
    mem,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use tauri::{ipc::Channel, Emitter, Manager, Runtime};

use opencv::core::Mat;

use crate::{
    errors::{SheetError, UploadError},
    image::{self, ProcessingState},
    scoring::{AnswerSheetResult, ScoreWeights},
};

pub type StateMutex = Mutex<AppState>;
pub static MODELS: OnceLock<PathBuf> = OnceLock::new();
#[macro_export]
macro_rules! signal {
    ($channel: ident, $message: expr) => {
        if let Err(e) = $channel.send($message) {
            log::error!("Channel emission failed: {e}");
        }
    };
}
macro_rules! emit_state {
    ($app: ident, $message: expr) => {
        if let Err(e) = $app.emit("state", $message) {
            log::error!("State event emission failed: {e}");
        }
    };
}

pub fn init_thread_ocr() -> Option<OcrEngine> {
    let model_path = MODELS.get()?;

    let patterns = model_path.join("tesseract.patterns");
    if !patterns.exists() {
        info!("Adding tesseract pattern file");
        std::fs::write(
            patterns,
            r#"\d\d\d\d\d\d\d\d\d
\A\d\d
\d\d\d
"#,
        )
        .inspect_err(|e| err_log!(e))
        .ok()?;
    }

    info!("Initializing thread OCR");
    OcrEngine::new(model_path.clone())
        .inspect_err(|e| err_log!(e))
        .ok()
}

#[derive(Default)]
pub struct AppState {
    state: AppStatePipeline,
    options: Options,
}

#[derive(Clone)]
pub enum MongoDB {
    Disable,
    Enable {
        mongo_db_uri: String,
        mongo_db_name: String,
    },
}

#[derive(Clone)]
pub struct Options {
    pub ocr: bool,
    pub mongo: MongoDB,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            ocr: true,
            mongo: MongoDB::Disable,
        }
    }
}

#[derive(Default)]
pub enum AppStatePipeline {
    #[default]
    Init,
    WithKey {
        key_image: Mat,
        key: AnswerKeySheet,
    },
    WithKeyAndWeights {
        key_image: Mat,
        key: AnswerKeySheet,
        weights: ScoreWeights,
    },
    Scoring {
        key_image: Mat,
        key: AnswerKeySheet,
        weights: ScoreWeights,
        processing_channel: tauri::async_runtime::Sender<ProcessingState>,
    },
    Scored {
        key_image: Mat,
        key: AnswerKeySheet,
        weights: ScoreWeights,
        answer_sheets: HashMap<String, (Mat, AnswerSheet, AnswerSheetResult)>,
    },
}
impl Display for AppStatePipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Init => "Init",
            Self::WithKey { .. } => "WithKey",
            Self::WithKeyAndWeights { .. } => "WithKeyAndWeights",
            Self::Scoring { .. } => "Scoring",
            Self::Scored { .. } => "Scored",
        })
    }
}

#[derive(Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginResponse {
    pub success: bool,
    pub error: Option<String>,
}

impl AppState {
    pub fn get_scored_answers<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
    ) -> Option<HashMap<String, (Mat, AnswerSheet, AnswerSheetResult)>> {
        let mutex = app.state::<StateMutex>();
        let state = mutex.lock().expect("poisoned");
        match &state.state {
            AppStatePipeline::Scored { answer_sheets, .. } => Some(answer_sheets.clone()),
            _ => None,
        }
    }
    pub fn upload_key<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: Channel<KeyUpload>,
        base64_image: Vec<u8>,
        image: Mat,
        key: AnswerKeySheet,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        match &mut state.state {
            AppStatePipeline::Init | AppStatePipeline::WithKey { .. } => {
                state.state = AppStatePipeline::WithKey {
                    key_image: image,
                    key,
                };
                signal!(
                    channel,
                    KeyUpload::Image {
                        bytes: base64_image
                    }
                );
            }
            AppStatePipeline::WithKeyAndWeights { weights, .. } => {
                if weights.weights.contains_key(&key.subject_id) {
                    state.state = AppStatePipeline::WithKeyAndWeights {
                        key_image: image,
                        key,
                        weights: mem::take(weights),
                    };
                } else {
                    state.state = AppStatePipeline::WithKey {
                        key_image: image,
                        key,
                    };
                    signal!(channel, KeyUpload::ClearWeights)
                }
                signal!(
                    channel,
                    KeyUpload::Image {
                        bytes: base64_image
                    }
                );
            }
            s => {
                error!("Unexpected state: {s}");
                signal!(
                    channel,
                    KeyUpload::Error {
                        error: format!("Unexpected state: {s}")
                    }
                )
            }
        }
        emit_state!(app, state.state.to_string());
    }
    pub fn upload_weights<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: &Channel<KeyUpload>,
        weights: ScoreWeights,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        match &mut state.state {
            AppStatePipeline::WithKey { key_image, key }
            | AppStatePipeline::WithKeyAndWeights { key_image, key, .. }
                if weights.weights.contains_key(&key.subject_id) =>
            {
                state.state = AppStatePipeline::WithKeyAndWeights {
                    key_image: mem::take(key_image),
                    key: mem::take(key),
                    weights,
                };
                signal!(channel, KeyUpload::UploadedWeights);
            }
            AppStatePipeline::WithKey { key, .. }
            | AppStatePipeline::WithKeyAndWeights { key, .. } => {
                error!(
                    "Cannot find weights mapping for subject ID {}",
                    key.subject_id
                );
                signal!(
                    channel,
                    KeyUpload::Error {
                        error: format!(
                            "Cannot find weights mapping for subject ID {}",
                            key.subject_id
                        )
                    }
                );
                signal!(channel, KeyUpload::MissingWeights);
            }
            s => {
                error!("Unexpected state: {s}");
                signal!(
                    channel,
                    KeyUpload::Error {
                        error: format!("Unexpected state: {s}")
                    }
                )
            }
        }
        emit_state!(app, state.state.to_string());
    }
    pub fn clear_key<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: &Channel<KeyUpload>,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        if let AppStatePipeline::WithKey { .. } = state.state {
            state.state = AppStatePipeline::Init;
            signal!(channel, KeyUpload::ClearImage);
        }
        emit_state!(app, state.state.to_string());
    }

    pub fn clear_weights<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: &Channel<KeyUpload>,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        if let AppStatePipeline::WithKeyAndWeights { key_image, key, .. } = &mut state.state {
            state.state = AppStatePipeline::WithKey {
                key_image: mem::take(key_image),
                key: mem::take(key),
            };
            signal!(channel, KeyUpload::ClearWeights);
        }
        emit_state!(app, state.state.to_string());
    }

    pub fn mark_scoring<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: &Channel<AnswerUpload>,
        images_count: usize,
        processing_channel: tauri::async_runtime::Sender<ProcessingState>,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");

        match &mut state.state {
            AppStatePipeline::WithKeyAndWeights {
                key_image,
                key,
                weights,
            }
            | AppStatePipeline::Scored {
                key_image,
                key,
                weights,
                ..
            } => {
                state.state = AppStatePipeline::Scoring {
                    key_image: mem::take(key_image),
                    key: mem::take(key),
                    weights: mem::take(weights),
                    processing_channel,
                };
                signal!(
                    channel,
                    AnswerUpload::Processing {
                        total: images_count,
                        started: 0,
                        finished: 0
                    }
                );
            }
            s => {
                error!("Unexpected state: {s}");
                signal!(
                    channel,
                    AnswerUpload::Error {
                        error: format!("Unexpected State: {s}")
                    }
                )
            }
        }
        emit_state!(app, state.state.to_string());
    }
    pub fn cancel_scoring<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: &Channel<AnswerUpload>,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        match &mut state.state {
            AppStatePipeline::Scoring {
                key_image,
                key,
                weights,
                processing_channel,
            } => {
                processing_channel
                    .blocking_send(ProcessingState::Cancel)
                    .expect("called in async context (impossible)");
                state.state = AppStatePipeline::WithKeyAndWeights {
                    key_image: mem::take(key_image),
                    key: mem::take(key),
                    weights: mem::take(weights),
                };
                signal!(channel, AnswerUpload::Cancelled);
            }
            s => {
                error!("Unexpected state: {s}");
                signal!(
                    channel,
                    AnswerUpload::Error {
                        error: format!("Unexpected state: {s}")
                    }
                )
            }
        }
        emit_state!(app, state.state.to_string());
    }

    pub fn upload_answer_sheets<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: &Channel<AnswerUpload>,
        result: Vec<image::ResultOfImageMatSheet>,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        match &mut state.state {
            AppStatePipeline::Scoring {
                key_image,
                key,
                weights,
                ..
            } => {
                signal!(channel, AnswerUpload::AlmostDone);
                type ImageMatSheetResultMaxScore =
                    (Vec<u8>, Mat, AnswerSheet, AnswerSheetResult, u32);
                let scored: Vec<Result<ImageMatSheetResultMaxScore, UploadError>> = result
                    .into_par_iter()
                    .map(|r| {
                        r.and_then(|t| {
                            weights.weights.get(&t.2.subject_id).cloned().map_or_else(
                                || Err(UploadError::MissingScoreWeights(t.2.clone().subject_id)),
                                |w| Ok((t.0, t.1, t.2.clone(), w.0, w.1)),
                            )
                        })
                        .map(|(s, mut m, a, w, ms)| {
                            let score = a.score(key, &w);
                            _ = score.write_score_marks(&mut m);
                            (s, m, a, score, ms)
                        })
                    })
                    .collect();
                let to_send: Vec<AnswerScoreResult> = scored
                    .par_iter()
                    .map(|r| match r {
                        Ok((
                            _,
                            mat,
                            AnswerSheet { student_id, .. },
                            AnswerSheetResult {
                                correct,
                                incorrect,
                                score,
                                ..
                            },
                            max_score,
                        )) => {
                            let img_small = image::resize_relative_img(mat, 0.4)
                                .and_then(|m| image::mat_to_webp(&m));
                            match img_small {
                                Ok(bytes) => AnswerScoreResult::Ok {
                                    student_id: student_id.clone(),
                                    bytes,
                                    score: *score,
                                    max_score: *max_score - weights.max_score_deduction(key),
                                    correct: *correct,
                                    incorrect: *incorrect,
                                },
                                Err(e) => {
                                    err_log!(&e);
                                    AnswerScoreResult::Error {
                                        error: format!("{e}"),
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            err_log!(e);
                            AnswerScoreResult::Error {
                                error: format!("{e}"),
                            }
                        }
                    })
                    .collect();
                let answer_sheets = scored
                    .into_par_iter()
                    .filter_map(|r| {
                        if let Ok((_, m, a, ca, _)) = r {
                            Some((a.student_id.clone(), (m, a, ca)))
                        } else {
                            None
                        }
                    })
                    .collect();
                state.state = AppStatePipeline::Scored {
                    key_image: mem::take(key_image),
                    key: mem::take(key),
                    weights: mem::take(weights),
                    answer_sheets,
                };
                signal!(channel, AnswerUpload::Done { uploaded: to_send });
            }
            s => {
                error!("Unexpected state: {s}");
                signal!(
                    channel,
                    AnswerUpload::Error {
                        error: format!("Unexpected state: {s}")
                    }
                )
            }
        }
        emit_state!(app, state.state.to_string());
    }
    pub fn clear_answer_sheets<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: &Channel<AnswerUpload>,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        if let AppStatePipeline::Scored {
            key,
            key_image,
            weights,
            ..
        } = &mut state.state
        {
            state.state = AppStatePipeline::WithKeyAndWeights {
                key_image: mem::take(key_image),
                key: mem::take(key),
                weights: mem::take(weights),
            };
            signal!(channel, AnswerUpload::Clear);
        }
        emit_state!(app, state.state.to_string());
    }
    pub fn set_ocr<R: Runtime, A: Emitter<R> + Manager<R>>(app: &A, ocr: bool) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        state.options.ocr = ocr;
    }
    pub fn get_options<R: Runtime, A: Emitter<R> + Manager<R>>(app: &A) -> Options {
        let mutex = app.state::<StateMutex>();
        let state = mutex.lock().expect("poisoned");
        state.options.clone()
    }
    pub fn set_mongodb<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        mongo_db_uri: String,
        mongo_db_name: String,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        let mongo_enum = MongoDB::Enable {
            mongo_db_uri,
            mongo_db_name,
        };
        state.options.mongo = mongo_enum;
    }

    pub fn get_base64_for_id<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        id: String,
    ) -> Option<Vec<u8>> {
        let mutex = app.state::<StateMutex>();
        let state = mutex.lock().expect("poisoned");
        if let AppStatePipeline::Scored { answer_sheets, .. } = &state.state {
            answer_sheets
                .get(&id)
                .and_then(|(mat, _, _)| image::mat_to_webp(mat).ok())
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnswerSheet {
    pub subject_id: String,
    pub student_id: String,
    pub subject_name: Option<String>,
    pub student_name: Option<String>,
    pub exam_room: Option<String>,
    pub exam_seat: Option<String>,
    pub answers: [QuestionGroup; 36],
}

#[derive(Debug, Clone)]
pub struct AnswerKeySheet {
    pub subject_id: String,
    pub answers: [QuestionGroup; 36],
}
impl From<AnswerSheet> for AnswerKeySheet {
    fn from(value: AnswerSheet) -> Self {
        Self {
            subject_id: value.subject_id,
            answers: value.answers,
        }
    }
}
impl Default for AnswerKeySheet {
    fn default() -> Self {
        Self {
            subject_id: String::default(),
            answers: array::from_fn(|_| QuestionGroup::default()),
        }
    }
}

#[allow(non_snake_case)]
#[derive(Default, Debug, Clone)]
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
pub enum Answer {
    Type(NumberType),
    Number(u8),
    Both(NumberType, u8),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberType {
    Plus,
    Minus,
    PlusOrMinus,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum KeyUpload {
    Cancelled,
    ClearImage,
    ClearWeights,
    UploadedWeights,
    MissingWeights,
    Image { bytes: Vec<u8> },
    Error { error: String },
}
#[derive(Clone, Serialize, Deserialize, Debug)]
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
    tag = "event",
    content = "data"
)]
pub enum CsvExport {
    Cancelled,
    Done,
    Error { error: String },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "result",
    content = "data"
)]
#[derive(Debug, PartialEq, Eq)]
pub enum AnswerScoreResult {
    Ok {
        student_id: String,
        bytes: Vec<u8>,
        score: u32,
        max_score: u32,
        correct: u32,
        incorrect: u32,
    },
    Error {
        error: String,
    },
}

#[cfg(test)]
pub mod unit_tests {
    use crate::image::upload_key_image_impl;
    use crate::image::upload_sheet_images_impl;
    use crate::scoring::upload_weights_impl;
    use std::fmt::Debug;
    use std::sync::Arc;
    use std::{path::PathBuf, sync::Mutex};

    use crate::state::StateMutex;

    use super::*;
    use opencv::core::{self, CMP_NE};
    use opencv::prelude::*;
    use serde::de::DeserializeOwned;
    use tauri::{test::MockRuntime, App, Manager};
    use tauri_plugin_fs::FilePath;

    pub fn mock_app_with_state(state: AppStatePipeline) -> App<MockRuntime> {
        let app = tauri::test::mock_app();
        app.manage(Mutex::new(AppState {
            state,
            options: Options {
                ocr: cfg!(feature = "ocr-tests"),
                mongo: MongoDB::Disable,
            },
        }));
        app
    }

    fn test_key_image() -> FilePath {
        FilePath::Path(PathBuf::from("tests/assets/sample_valid_image.jpg"))
    }

    fn test_weights() -> Vec<FilePath> {
        vec![
            FilePath::Path(PathBuf::from("tests/assets/weights.csv")),
            FilePath::Path(PathBuf::from("tests/assets/weights2.csv")),
            FilePath::Path(PathBuf::from("tests/assets/weights3.csv")),
        ]
    }

    fn test_images() -> Vec<FilePath> {
        vec![
            FilePath::Path(PathBuf::from("tests/assets/image_001.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/image_002.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/image_003.jpg")),
            FilePath::Path(PathBuf::from("tests/assets/image_004.jpg")),
        ]
    }

    fn not_image() -> FilePath {
        FilePath::Path(PathBuf::from("tests/assets/sample_invalid_image.jpg"))
    }

    fn setup_ocr_data() {
        _ = MODELS.set(PathBuf::from("tests/assets"))
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

    fn setup_channel_msgs<T: Debug + DeserializeOwned + Send + Sync + 'static>(
    ) -> (Channel<T>, Arc<Mutex<Vec<T>>>) {
        let channel_msgs = Arc::new(Mutex::new(Vec::<T>::new()));
        let channel_msgs_ref = Arc::clone(&channel_msgs);
        (
            Channel::new(move |msg| {
                let mut vec = channel_msgs_ref.lock().unwrap();
                let msg: T = msg.deserialize().unwrap();
                let mut fmt = format!("got message: {msg:?}");
                fmt.truncate(200);
                println!("{fmt}");
                vec.push(msg);
                Ok(())
            }),
            channel_msgs,
        )
    }

    macro_rules! assert_state {
        ($app: ident, $pattern:pat $(if $guard:expr)? $(,)?) => {{
            let mutex = $app.state::<StateMutex>();
            let state = mutex.lock().unwrap();
            assert!(matches!(state.state, $pattern $(if $guard)?));
        }};
    }
    macro_rules! unwrap_msgs {
        ($msgs: ident) => {
            $msgs.lock().unwrap()
        };
    }

    #[test]
    fn test_app_key_upload() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (channel, msgs) = setup_channel_msgs::<KeyUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), channel);

        assert_state!(app, AppStatePipeline::WithKey { .. });
        let msg_history = unwrap_msgs!(msgs);
        assert!(matches!(msg_history[0], KeyUpload::Image { .. }))
    }
    #[test]
    fn test_app_change_key_upload() {
        setup_ocr_data();
        let path = test_key_image();
        let path2 = test_images().remove(1);

        let app = mock_app_with_state(AppStatePipeline::Init);
        let (channel, msgs) = setup_channel_msgs::<KeyUpload>();

        upload_key_image_impl(&app, Some(path), channel.clone());

        let current_mat = {
            let mutex = app.state::<StateMutex>();
            let state = mutex.lock().expect("poisoned");
            let AppStatePipeline::WithKey { key_image, .. } = &state.state else {
                unreachable!()
            };
            key_image.clone()
        };

        upload_key_image_impl(&app, Some(path2), channel);

        let mutex = app.state::<StateMutex>();
        let state = mutex.lock().unwrap();
        if let AppStatePipeline::WithKey { key_image, .. } = &state.state {
            assert!(!compare_mats(key_image, &current_mat));
        } else {
            unreachable!()
        }

        let msgs = unwrap_msgs!(msgs);
        let mut msgs = msgs.iter();
        assert!(matches!(msgs.next(), Some(KeyUpload::Image { .. })));
        assert!(matches!(msgs.next(), Some(KeyUpload::Image { .. })));
    }
    #[test]
    fn test_app_key_canceled_upload() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (channel, msgs) = setup_channel_msgs::<KeyUpload>();
        upload_key_image_impl(&app, None, channel);

        assert_state!(app, AppStatePipeline::Init);
        let msgs = unwrap_msgs!(msgs);
        let mut msgs = msgs.iter();
        assert!(matches!(msgs.next(), Some(KeyUpload::Cancelled)));
    }
    #[test]
    fn test_app_key_invalid_upload() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (channel, msgs) = setup_channel_msgs::<KeyUpload>();
        upload_key_image_impl(&app, Some(not_image()), channel);

        assert_state!(app, AppStatePipeline::Init);
        let msgs = unwrap_msgs!(msgs);
        let mut msgs = msgs.iter();
        assert!(matches!(msgs.next(), Some(KeyUpload::Error { .. })));
    }
    #[test]
    fn test_app_key_clear() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (channel, msgs) = setup_channel_msgs::<KeyUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), channel.clone());

        assert_state!(app, AppStatePipeline::WithKey { .. });

        AppState::clear_key(&app, &channel);

        assert_state!(app, AppStatePipeline::Init);
        let msgs = unwrap_msgs!(msgs);
        let mut msgs = msgs.iter();
        assert!(matches!(msgs.next(), Some(KeyUpload::Image { .. })));
        assert!(matches!(msgs.next(), Some(KeyUpload::ClearImage)));
    }

    #[test]
    fn test_app_weights_upload() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (channel, msgs) = setup_channel_msgs::<KeyUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), channel.clone());
        upload_weights_impl(&app, Some(test_weights().remove(0)), channel);

        assert_state!(app, AppStatePipeline::WithKeyAndWeights { .. });
        let msg_history = unwrap_msgs!(msgs);
        assert!(matches!(msg_history[0], KeyUpload::Image { .. }));
        assert!(matches!(msg_history[1], KeyUpload::UploadedWeights));
    }
    #[test]
    fn test_app_change_weights_upload() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (channel, msgs) = setup_channel_msgs::<KeyUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), channel.clone());
        upload_weights_impl(&app, Some(test_weights().remove(0)), channel.clone());

        let current_weights_1 = {
            let mutex = app.state::<StateMutex>();
            let state = mutex.lock().expect("poisoned");
            let AppStatePipeline::WithKeyAndWeights { weights, .. } = &state.state else {
                unreachable!()
            };
            weights.clone()
        };

        upload_weights_impl(&app, Some(test_weights().remove(1)), channel);

        let current_weights_2 = {
            let mutex = app.state::<StateMutex>();
            let state = mutex.lock().expect("poisoned");
            let AppStatePipeline::WithKeyAndWeights { weights, .. } = &state.state else {
                unreachable!()
            };
            weights.clone()
        };

        assert_ne!(current_weights_1, current_weights_2);
        let msg_history = unwrap_msgs!(msgs);
        assert!(matches!(msg_history[0], KeyUpload::Image { .. }));
        assert!(matches!(msg_history[1], KeyUpload::UploadedWeights));
        assert!(matches!(msg_history[2], KeyUpload::UploadedWeights));
    }
    #[test]
    fn test_app_weights_canceled_upload() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (channel, msgs) = setup_channel_msgs::<KeyUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), channel.clone());
        upload_weights_impl(&app, None, channel);

        assert_state!(app, AppStatePipeline::WithKey { .. });
        let msg_history = unwrap_msgs!(msgs);
        assert!(matches!(msg_history[0], KeyUpload::Image { .. }));
        assert!(matches!(msg_history[1], KeyUpload::Cancelled));
    }
    #[test]
    fn test_app_weights_clear() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (channel, msgs) = setup_channel_msgs::<KeyUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), channel.clone());
        upload_weights_impl(&app, Some(test_weights().remove(0)), channel.clone());

        assert_state!(app, AppStatePipeline::WithKeyAndWeights { .. });

        AppState::clear_weights(&app, &channel);

        assert_state!(app, AppStatePipeline::WithKey { .. });
        let msgs = unwrap_msgs!(msgs);
        let mut msgs = msgs.iter();
        assert!(matches!(msgs.next(), Some(KeyUpload::Image { .. })));
        assert!(matches!(msgs.next(), Some(KeyUpload::UploadedWeights)));
        assert!(matches!(msgs.next(), Some(KeyUpload::ClearWeights)));
    }
    #[test]
    fn test_app_different_weights_upload() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (channel, msgs) = setup_channel_msgs::<KeyUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), channel.clone());
        upload_weights_impl(&app, Some(test_weights().remove(2)), channel.clone());

        assert_state!(app, AppStatePipeline::WithKey { .. });
        let msgs = unwrap_msgs!(msgs);
        let mut msgs = msgs.iter();
        assert!(matches!(msgs.next(), Some(KeyUpload::Image { .. })));
        assert!(matches!(msgs.next(), Some(KeyUpload::Error { .. })));
        assert!(matches!(msgs.next(), Some(KeyUpload::MissingWeights)));
    }
    #[test]
    fn test_app_weights_key_clear_same() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (channel, msgs) = setup_channel_msgs::<KeyUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), channel.clone());
        upload_weights_impl(&app, Some(test_weights().remove(0)), channel.clone());

        // upload another key in same subject
        upload_key_image_impl(&app, Some(test_images().remove(1)), channel.clone());

        assert_state!(app, AppStatePipeline::WithKeyAndWeights { .. });
        let msgs = unwrap_msgs!(msgs);
        let mut msgs = msgs.iter();
        assert!(matches!(msgs.next(), Some(KeyUpload::Image { .. })));
        assert!(matches!(msgs.next(), Some(KeyUpload::UploadedWeights)));
        assert!(matches!(msgs.next(), Some(KeyUpload::Image { .. })));
    }
    #[test]
    fn test_app_weights_key_clear_different() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (channel, msgs) = setup_channel_msgs::<KeyUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), channel.clone());
        upload_weights_impl(&app, Some(test_weights().remove(0)), channel.clone());

        // upload another key in different subject
        upload_key_image_impl(&app, Some(test_images().remove(3)), channel.clone());

        assert_state!(app, AppStatePipeline::WithKey { .. });
        let msgs = unwrap_msgs!(msgs);
        let mut msgs = msgs.iter();
        assert!(matches!(msgs.next(), Some(KeyUpload::Image { .. })));
        assert!(matches!(msgs.next(), Some(KeyUpload::UploadedWeights)));
        assert!(matches!(msgs.next(), Some(KeyUpload::ClearWeights)));
        assert!(matches!(msgs.next(), Some(KeyUpload::Image { .. })));
    }

    #[test]
    fn test_app_sheets_upload() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (key_channel, _) = setup_channel_msgs::<KeyUpload>();
        let (sheet_channel, sheet_msgs) = setup_channel_msgs::<AnswerUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), key_channel.clone());
        upload_weights_impl(&app, Some(test_weights().remove(0)), key_channel);
        upload_sheet_images_impl(&app, Some(test_images()), sheet_channel);

        assert_state!(app, AppStatePipeline::Scored { .. });

        let msgs = unwrap_msgs!(sheet_msgs);
        let mut msgs = msgs
            .iter()
            .filter(|a| !matches!(a, AnswerUpload::Processing { .. }));

        assert!(matches!(msgs.next(), Some(AnswerUpload::AlmostDone)));
        assert!(matches!(msgs.next(), Some(AnswerUpload::Done { .. })));
    }
    #[test]
    fn test_app_change_sheets_upload() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (key_channel, _) = setup_channel_msgs::<KeyUpload>();
        let (sheet_channel, sheet_msgs) = setup_channel_msgs::<AnswerUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), key_channel.clone());
        upload_weights_impl(&app, Some(test_weights().remove(0)), key_channel);
        upload_sheet_images_impl(&app, Some(test_images()), sheet_channel.clone());

        let current_count = {
            let mutex = app.state::<StateMutex>();
            let state = mutex.lock().expect("poisoned");
            let AppStatePipeline::Scored { answer_sheets, .. } = &state.state else {
                unreachable!()
            };
            answer_sheets.len()
        };

        upload_sheet_images_impl(&app, Some(vec![test_images().remove(0)]), sheet_channel);

        let mutex = app.state::<StateMutex>();
        let state = mutex.lock().unwrap();
        if let AppStatePipeline::Scored { answer_sheets, .. } = &state.state {
            assert_ne!(current_count, answer_sheets.len());
        } else {
            unreachable!()
        }

        let msgs = unwrap_msgs!(sheet_msgs);
        let mut msgs = msgs
            .iter()
            .filter(|a| !matches!(a, AnswerUpload::Processing { .. }));
        assert!(matches!(msgs.next(), Some(AnswerUpload::AlmostDone)));
        assert!(matches!(msgs.next(), Some(AnswerUpload::Done { .. })));
        assert!(matches!(msgs.next(), Some(AnswerUpload::AlmostDone)));
        assert!(matches!(msgs.next(), Some(AnswerUpload::Done { .. })));
    }
    #[test]
    fn test_app_sheets_canceled_upload() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (key_channel, _) = setup_channel_msgs::<KeyUpload>();
        let (sheet_channel, sheet_msgs) = setup_channel_msgs::<AnswerUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), key_channel.clone());
        upload_weights_impl(&app, Some(test_weights().remove(0)), key_channel);
        upload_sheet_images_impl(&app, None, sheet_channel);

        assert_state!(app, AppStatePipeline::WithKeyAndWeights { .. });

        let msgs = unwrap_msgs!(sheet_msgs);
        let mut msgs = msgs.iter();
        assert!(matches!(msgs.next(), Some(AnswerUpload::Cancelled)));
    }
    #[test]
    fn test_app_sheets_invalid_upload() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (key_channel, _) = setup_channel_msgs::<KeyUpload>();
        let (sheet_channel, sheet_msgs) = setup_channel_msgs::<AnswerUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), key_channel.clone());
        upload_weights_impl(&app, Some(test_weights().remove(0)), key_channel);
        upload_sheet_images_impl(&app, Some(vec![not_image()]), sheet_channel);

        {
            let mutex = app.state::<StateMutex>();
            let state = mutex.lock().unwrap();
            let AppStatePipeline::Scored { answer_sheets, .. } = &state.state else {
                unreachable!()
            };

            assert_eq!(answer_sheets.len(), 0);
        };

        let msgs = unwrap_msgs!(sheet_msgs);
        let mut msgs = msgs
            .iter()
            .filter(|a| !matches!(a, AnswerUpload::Processing { .. }));

        assert!(matches!(msgs.next(), Some(AnswerUpload::AlmostDone)));
        let Some(AnswerUpload::Done { uploaded }) = msgs.next() else {
            unreachable!()
        };
        assert!(matches!(uploaded[0], AnswerScoreResult::Error { .. }));
    }
    #[test]
    fn test_app_sheets_clear() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (key_channel, _) = setup_channel_msgs::<KeyUpload>();
        let (sheet_channel, sheet_msgs) = setup_channel_msgs::<AnswerUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), key_channel.clone());
        upload_weights_impl(&app, Some(test_weights().remove(0)), key_channel);
        upload_sheet_images_impl(&app, Some(test_images()), sheet_channel.clone());

        assert_state!(app, AppStatePipeline::Scored { .. });

        AppState::clear_answer_sheets(&app, &sheet_channel);

        assert_state!(app, AppStatePipeline::WithKeyAndWeights { .. });

        let msgs = unwrap_msgs!(sheet_msgs);
        let mut msgs = msgs
            .iter()
            .filter(|a| !matches!(a, AnswerUpload::Processing { .. }));

        assert!(matches!(msgs.next(), Some(AnswerUpload::AlmostDone)));
        assert!(matches!(msgs.next(), Some(AnswerUpload::Done { .. })));

        assert!(matches!(msgs.next(), Some(AnswerUpload::Clear)));
    }

    #[test]
    fn test_clear_key_on_scored_does_nothing() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (key_channel, _) = setup_channel_msgs::<KeyUpload>();
        let (sheet_channel, _) = setup_channel_msgs::<AnswerUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), key_channel.clone());
        upload_weights_impl(&app, Some(test_weights().remove(0)), key_channel.clone());
        upload_sheet_images_impl(&app, Some(test_images()), sheet_channel);

        assert_state!(app, AppStatePipeline::Scored { .. });

        AppState::clear_key(&app, &key_channel);

        // Should still be in Scored
        assert_state!(app, AppStatePipeline::Scored { .. });
    }
    #[test]
    fn test_clear_answer_sheets_on_init_does_nothing() {
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (sheet_channel, sheet_msgs) = setup_channel_msgs::<AnswerUpload>();
        AppState::clear_answer_sheets(&app, &sheet_channel);
        assert_state!(app, AppStatePipeline::Init);

        let msgs = unwrap_msgs!(sheet_msgs);
        assert!(msgs.is_empty());
    }
    #[test]
    fn test_clear_answer_sheets_on_with_key_does_nothing() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (key_channel, _) = setup_channel_msgs::<KeyUpload>();
        let (sheet_channel, _) = setup_channel_msgs::<AnswerUpload>();
        upload_key_image_impl(&app, Some(test_key_image()), key_channel);

        assert_state!(app, AppStatePipeline::WithKey { .. });

        AppState::clear_answer_sheets(&app, &sheet_channel);

        assert_state!(app, AppStatePipeline::WithKey { .. });
    }
    #[test]
    fn test_upload_sheets_without_key_does_nothing() {
        setup_ocr_data();
        let app = mock_app_with_state(AppStatePipeline::Init);
        let (sheet_channel, _) = setup_channel_msgs::<AnswerUpload>();
        upload_sheet_images_impl(&app, Some(test_images()), sheet_channel);

        // Should remain in Init because upload_sheets does nothing without a key
        assert_state!(app, AppStatePipeline::Init);
    }
}
