use std::sync::Mutex;

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
