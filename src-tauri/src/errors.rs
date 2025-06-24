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
    #[error("Tesseract Error: {0}")]
    TesseractError(#[from] tesseract_rs::TesseractError)
}
