use std::fmt::Write;

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
    #[error("OCR error: {0}")]
    OcrError(#[from] OcrError),
    #[error("Incomplete markers on page")]
    MissingMarkers,
}

#[derive(thiserror::Error, Debug)]
pub enum ModelDownloadError {
    #[error("Unsupported Operating System (cannot determine cache dir)")]
    CacheDirUnknown,
    #[error("I/O error while trying to access models: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Cannot make network request: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Cannot convert header to string: {0}")]
    ToStrError(#[from] reqwest::header::ToStrError),
    #[error("Cannot convert header string to number: {0}")]
    ParseIntError(#[from] std::num::ParseIntError),
    #[error("Response is missing content length")]
    NoContentLength,
}

impl serde::Serialize for ModelDownloadError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

pub fn fmt_error_chain_of(mut err: &dyn std::error::Error) -> String {
    let mut str = err.to_string();
    while let Some(src) = err.source() {
        _ = write!(str, "\n  -> Caused by {src}");
        err = src;
    }
    str
}

#[macro_export]
macro_rules! err_log {
    ($error: expr) => {
        log::error!("{}", $crate::errors::fmt_error_chain_of($error))
    };
}

#[derive(thiserror::Error, Debug)]
pub enum CsvError {
    #[error("Invalid path: {0}")]
    InvalidPath(#[from] tauri_plugin_fs::Error),
    #[error("Cannot open/write file: {0}")]
    FileOperationFailed(#[from] std::io::Error),
    #[error("Tried to export CSV while in an incorrect state. This is a bug.")]
    IncorrectState,
    #[error("Failed to serialize CSV: {0}")]
    Csv(#[from] csv::Error),
}
#[derive(thiserror::Error, Debug)]
pub enum DatabaseError {
    //#[error("Environment variable MONGO_URI cannot be found. (reason: {0}) Please declare it before starting the program or provide it via an .env file.")]
    //MissingMongoUri(#[source] std::env::VarError),
    //#[error("Environment variable MY_DATABASE cannot be found. (reason: {0}) Please declare it before starting the program or provide it via an .env file.")]
    //MissingDbName(#[source] std::env::VarError),
    #[error("MongoDB error: {0}")]
    MongoDb(#[from] mongodb::error::Error),
}

/// Wrapper for Tesseract errors that happen at different stages.
#[derive(thiserror::Error, Debug)]
pub enum OcrError {
    #[error(
        "No `tesseract` command found in PATH. Please install tesseract first before using OCR."
    )]
    NoTesseract,
    #[error("tessdata path is not unicode. somehow.")]
    NoUnicode,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid image size: {0}")]
    InvalidSize(#[from] std::num::TryFromIntError),
    #[error("Cannot encode image: {0}")]
    InvalidImage(#[from] opencv::Error),
}
