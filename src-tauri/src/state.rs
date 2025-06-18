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
        _sheet_images: Vec<Mat>,
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
        let mut state = mutex.lock().unwrap();
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
        let mut state = mutex.lock().unwrap();
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
                *state = AppState::WithKeyAndSheets {
                    key_image: key_image.clone(),
                    // key: key.clone(),
                    _sheet_images: images,
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
        let mut state = mutex.lock().unwrap();
        if let AppState::WithKeyAndSheets {
            /*key,*/ ref key_image,
            ..
        } = *state
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
