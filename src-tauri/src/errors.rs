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

#[derive(thiserror::Error, Debug)]
pub enum CsvError {
    #[error("Invalid path: {0}")]
    InvalidPath(#[from] tauri_plugin_fs::Error),
    #[error("Non UTF-8 path")]
    NonUtfPath,
    #[error("Cannot open file: {0}")] // TODO: use format_error_trace when merged with main
    FileOpenFailed(#[from] std::io::Error),
    #[error("Tried to export CSV while in an incorrect state. This is a bug.")]
    IncorrectState,
}
