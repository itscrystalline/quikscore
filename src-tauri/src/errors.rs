#[derive(thiserror::Error, Debug)]
pub enum UploadError {
    #[error("User canceled upload")]
    Canceled,
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
}

#[derive(thiserror::Error, Debug)]
pub enum SheetError {
    #[error("OpenCV Error: {} (errno {})", .0.message, .0.code)]
    OpenCvError(#[from] opencv::Error),
    #[error("Detected less than 5 answers (this should not happen)")]
    TooLittleAnswers,
    #[error("Detected less than 36 groups (this should not happen)")]
    TooLittleGroups,
    #[error("Unimplemented")]
    Unimplemented,
}
