#[derive(thiserror::Error, Debug)]
pub enum UploadError {
    #[error("Invalid path: {0}")]
    InvalidPath(#[from] tauri_plugin_fs::Error),
    #[error("Non UTF-8 path")]
    NonUtfPath,
    #[error("Invalid image format")]
    NotImage,
    #[error("Unable to reencode image: {} (errno {})", .0.message, .0.code)]
    EncodeError(#[from] opencv::Error),
    #[error("Unable to detect answer sheet: {0}")]
    NotAnswerSheet(#[from] SheetError),
    #[error("Pipe between processing threads and main thread unexpectetly broken")]
    UnexpectedPipeClosure,
    #[error("Weights file does not contain weights for subject id {0}")]
    MissingScoreWeights(String),
    #[error("Processing has been prematurely cancelled")]
    PrematureCancellaton,
}

#[derive(thiserror::Error, Debug)]
pub enum SheetError {
    #[error("OpenCV Error: {} (errno {})", .0.message, .0.code)]
    OpenCvError(#[from] opencv::Error),
    #[error("Detected less than 5 answers (this should not happen)")]
    TooLittleAnswers,
    #[error("Anyhow error")]
    OcrError(#[from] anyhow::Error),
    // #[error("Tesseract Error: {0}")]
    // TesseractError(#[from] tesseract_rs::TesseractError),
}
