use core::hash;
use ocrs::OcrEngine;
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fmt::Display,
    fs::File,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use tauri::{ipc::Channel, Emitter, Manager, Runtime};
// use tesseract_rs::TesseractAPI;

use opencv::core::Mat;

use crate::{
    errors::{SheetError, UploadError},
    image::{self, ProcessingState},
    scoring::{AnswerSheetResult, ScoreWeights},
};

pub type StateMutex = Mutex<AppState>;
pub static MODELS: OnceLock<PathBuf> = OnceLock::new();

const TEXT_DETECTION_HASH: &str =
    "f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca";
const TEXT_RECOGNITION_HASH: &str =
    "e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e";
pub fn get_or_download_models(frontend_channel: Channel<ModelDownload>) -> Result<(), String> {
    let mut cache_dir = dirs::cache_dir().ok_or("unsupported operating system".to_string())?;
    cache_dir.push("quikscore");

    let detection_model = cache_dir.join("text-detection.rten");
    let recognition_model = cache_dir.join("text-recognition.rten");

    let detection_model_exists = detection_model
        .try_exists()
        .map_err(|e| format!("error while trying to locate detection model: {e}"))?;
    let recognition_model_exists = recognition_model
        .try_exists()
        .map_err(|e| format!("error while trying to locate detection model: {e}"))?;

    let mut hasher = Sha256::new();
    let need_download_detection = if detection_model_exists {
        let mut detection_model_file = File::open(detection_model)
            .map_err(|e| format!("error opening detection model file for hashing: {e}"))?;
        _ = std::io::copy(&mut detection_model_file, &mut hasher)
            .map_err(|e| format!("error hashing detection model: {e}"))?;
        hasher.finalize() != TEXT_DETECTION_HASH
    } else {
        true
    };
    hasher.reset();
    let need_download_recognition = if recognition_model_exists {
        let mut recognition_model_file = File::open(recognition_model)
            .map_err(|e| format!("error opening recognition model file for hashing: {e}"))?;
        _ = std::io::copy(&mut recognition_model_file, &mut hasher)
            .map_err(|e| format!("error hashing recognition model: {e}"))?;
        hasher.finalize() != TEXT_RECOGNITION_HASH
    } else {
        true
    };

    todo!();

    Ok(())
}
pub fn init_thread_ocr() -> Option<OcrEngine> {
    let model_path = MODELS.get()?;
    let detection_model = model_path.join("text-detection.rten");
    let recognition_model = model_path.join("text-recognition.rten");
    println!("initializing thread OcrEngine");

    let detection = rten::Model::load_file(detection_model)
        .inspect_err(|e| println!("error loading detection model: {e}"))
        .ok()?;
    let recognition = rten::Model::load_file(recognition_model)
        .inspect_err(|e| println!("error loading recognition model: {e}"))
        .ok()?;

    OcrEngine::new(ocrs::OcrEngineParams {
        detection_model: Some(detection),
        recognition_model: Some(recognition),
        ..Default::default()
    })
    .ok()
}

#[macro_export]
macro_rules! signal {
    ($channel: ident, $message: expr) => {
        if let Err(e) = $channel.send($message) {
            println!("Channel emission failed: {e}");
        }
    };
}
macro_rules! emit_state {
    ($app: ident, $message: expr) => {
        if let Err(e) = $app.emit("state", $message) {
            println!("State event emission failed: {e}");
        }
    };
}

#[derive(Default)]
pub struct AppState {
    state: AppStatePipeline,
    options: Options,
}

#[derive(Copy, Clone)]
pub struct Options {
    pub ocr: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self { ocr: true }
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
        _answer_sheets: HashMap<String, (Mat, AnswerSheet, AnswerSheetResult)>,
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

impl AppState {
    pub fn upload_key<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: Channel<KeyUpload>,
        base64_image: String,
        image: Mat,
        key: AnswerKeySheet,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        match &state.state {
            AppStatePipeline::Init | AppStatePipeline::WithKey { .. } => {
                state.state = AppStatePipeline::WithKey {
                    key_image: image,
                    key,
                };
                signal!(
                    channel,
                    KeyUpload::Image {
                        base64: base64_image
                    }
                );
            }
            AppStatePipeline::WithKeyAndWeights { weights, .. } => {
                if weights.weights.contains_key(&key.subject_code) {
                    state.state = AppStatePipeline::WithKeyAndWeights {
                        key_image: image,
                        key,
                        weights: weights.clone(),
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
                        base64: base64_image
                    }
                );
            }
            s => signal!(
                channel,
                KeyUpload::Error {
                    error: format!("Unexpected state: {s}")
                }
            ),
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
        match &state.state {
            AppStatePipeline::WithKey { key_image, key }
            | AppStatePipeline::WithKeyAndWeights { key_image, key, .. }
                if weights.weights.contains_key(&key.subject_code) =>
            {
                state.state = AppStatePipeline::WithKeyAndWeights {
                    key_image: key_image.clone(),
                    key: key.clone(),
                    weights,
                };
                signal!(channel, KeyUpload::UploadedWeights);
            }
            AppStatePipeline::WithKey { key, .. }
            | AppStatePipeline::WithKeyAndWeights { key, .. } => {
                signal!(
                    channel,
                    KeyUpload::Error {
                        error: format!(
                            "Cannot find weights mapping for subject ID {}",
                            key.subject_code
                        )
                    }
                );
                signal!(channel, KeyUpload::MissingWeights);
            }
            s => signal!(
                channel,
                KeyUpload::Error {
                    error: format!("Unexpected state: {s}")
                }
            ),
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
        if let AppStatePipeline::WithKeyAndWeights { key_image, key, .. } = &state.state {
            state.state = AppStatePipeline::WithKey {
                key_image: key_image.clone(),
                key: key.clone(),
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

        match &state.state {
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
                    key_image: key_image.clone(),
                    key: key.clone(),
                    weights: weights.clone(),
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
            s => signal!(
                channel,
                AnswerUpload::Error {
                    error: format!("Unexpected State: {s}")
                }
            ),
        }
        emit_state!(app, state.state.to_string());
    }
    pub fn cancel_scoring<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: &Channel<AnswerUpload>,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        match &state.state {
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
                    key_image: key_image.clone(),
                    key: key.clone(),
                    weights: weights.clone(),
                };
                signal!(channel, AnswerUpload::Cancelled);
            }
            s => signal!(
                channel,
                AnswerUpload::Error {
                    error: format!("Unexpected state: {s}")
                }
            ),
        }
        emit_state!(app, state.state.to_string());
    }

    pub fn upload_answer_sheets<R: Runtime, A: Emitter<R> + Manager<R>>(
        app: &A,
        channel: &Channel<AnswerUpload>,
        result: Vec<Result<(String, Mat, AnswerSheet), UploadError>>,
    ) {
        let mutex = app.state::<StateMutex>();
        let mut state = mutex.lock().expect("poisoned");
        match &state.state {
            AppStatePipeline::Scoring {
                key_image,
                key,
                weights,
                ..
            } => {
                signal!(channel, AnswerUpload::AlmostDone);
                type Base64MatSheetResultMaxScore =
                    (String, Mat, AnswerSheet, AnswerSheetResult, u32);
                let scored: Vec<Result<Base64MatSheetResultMaxScore, UploadError>> = result
                    .into_par_iter()
                    .map(|r| {
                        r.and_then(|t| {
                            weights.weights.get(&t.2.subject_code).cloned().map_or_else(
                                || Err(UploadError::MissingScoreWeights(t.2.clone().subject_code)),
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
                                not_answered,
                                score,
                                ..
                            },
                            max_score,
                        )) => {
                            let img_small = image::resize_relative_img(mat, 0.4)
                                .and_then(|m| image::mat_to_base64_png(&m));
                            match img_small {
                                Ok(base64) => AnswerScoreResult::Ok {
                                    student_id: student_id.clone(),
                                    base64,
                                    score: *score,
                                    max_score: *max_score,
                                    correct: *correct,
                                    incorrect: *incorrect,
                                    not_answered: *not_answered,
                                },
                                Err(e) => AnswerScoreResult::Error {
                                    error: format!("{e}"),
                                },
                            }
                        }
                        Err(e) => AnswerScoreResult::Error {
                            error: format!("{e}"),
                        },
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
                    key_image: key_image.clone(),
                    key: key.clone(),
                    weights: weights.clone(),
                    _answer_sheets: answer_sheets,
                };
                signal!(channel, AnswerUpload::Done { uploaded: to_send });
            }
            s => signal!(
                channel,
                AnswerUpload::Error {
                    error: format!("Unexpected state: {s}")
                }
            ),
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
        } = &state.state
        {
            state.state = AppStatePipeline::WithKeyAndWeights {
                key_image: key_image.clone(),
                key: key.clone(),
                weights: weights.clone(),
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
        state.options
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
pub enum ModelDownload {
    Progress {
        progress_detection: u32,
        progress_recognition: u32,
        total: u32,
    },
    Error {
        error: String,
    },
    Success,
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
    ClearImage,
    ClearWeights,
    UploadedWeights,
    MissingWeights,
    Image { base64: String },
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
#[derive(Debug, PartialEq, Eq)]
pub enum AnswerScoreResult {
    Ok {
        student_id: String,
        base64: String,
        score: u32,
        max_score: u32,
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
    use crate::scoring::upload_weights_impl;
    use std::sync::Arc;
    use std::{path::PathBuf, sync::Mutex};

    use crate::state::StateMutex;

    use super::*;
    use opencv::core::{self, CMP_NE};
    use opencv::prelude::*;
    use serde::de::DeserializeOwned;
    use tauri::{test::MockRuntime, App, Manager};
    use tauri_plugin_fs::FilePath;

    fn mock_app_with_state(state: AppStatePipeline) -> App<MockRuntime> {
        let app = tauri::test::mock_app();
        app.manage(Mutex::new(AppState {
            state,
            options: Options {
                ocr: cfg!(feature = "ocr-tests"),
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
        get_or_download_models(Some(PathBuf::from("tests/assets")))
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

    fn setup_channel_msgs<T: DeserializeOwned + Send + Sync + 'static>(
    ) -> (Channel<T>, Arc<Mutex<Vec<T>>>) {
        let channel_msgs = Arc::new(Mutex::new(Vec::<T>::new()));
        let channel_msgs_ref = Arc::clone(&channel_msgs);
        (
            Channel::new(move |msg| {
                let mut vec = channel_msgs_ref.lock().unwrap();
                let msg: T = msg.deserialize().unwrap();
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
            let AppStatePipeline::Scored {
                _answer_sheets: answer_sheets,
                ..
            } = &state.state
            else {
                unreachable!()
            };
            answer_sheets.len()
        };

        upload_sheet_images_impl(&app, Some(vec![test_images().remove(0)]), sheet_channel);

        let mutex = app.state::<StateMutex>();
        let state = mutex.lock().unwrap();
        if let AppStatePipeline::Scored {
            _answer_sheets: answer_sheets,
            ..
        } = &state.state
        {
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
            let AppStatePipeline::Scored {
                _answer_sheets: answer_sheets,
                ..
            } = &state.state
            else {
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
