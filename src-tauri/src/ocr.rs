use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

use opencv::{
    core::{Size_, Vector},
    imgcodecs::imencode_def,
    prelude::*,
};

use crate::errors::OcrError;

/// An `ocrs::OcrEngine`-like wrapper for tesseract, provided by ~~opencv::text::OCRTesseract~~ THE
/// FUCKING TESSERACT COMMAND I'M TIRED OF LINKING ISSUES.
pub struct OcrEngine {
    tessdata_path: PathBuf,
}
pub struct ImageSource(Vec<u8>);

impl OcrEngine {
    pub fn check_tesseract() -> Result<bool, OcrError> {
        let tess = Command::new("tesseract")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        Ok(tess.success())
    }
    pub fn new(datapath: PathBuf) -> Result<OcrEngine, OcrError> {
        if !OcrEngine::check_tesseract()? {
            return Err(OcrError::NoTesseract);
        }
        Ok(OcrEngine {
            tessdata_path: datapath,
        })
    }
    /// Q: whats the point of this
    /// A: mraow :3c
    #[inline(always)]
    pub fn prepare_input(&self, src: ImageSource) -> Result<ImageSource, OcrError> {
        Ok(src)
    }
    pub fn get_text(&self, src: ImageSource) -> Result<String, OcrError> {
        let mut tesseract = Command::new("tesseract")
            .arg("stdin")
            .arg("stdout")
            .args(["-l", "eng"])
            .args(["--loglevel", "OFF"])
            .args(["--psm", "single_line"])
            .args([
                "--user-patterns",
                self.tessdata_path
                    .join("tesseract.patterns")
                    .to_str()
                    .ok_or(OcrError::NoUnicode)?,
            ])
            .args([
                "--tessdata-dir",
                self.tessdata_path.to_str().ok_or(OcrError::NoUnicode)?,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        _ = tesseract
            .stdin
            .take()
            .expect("no stdin????")
            .write(&src.0)?;

        let Output { stdout, .. } = tesseract.wait_with_output()?;

        Ok(String::from_utf8_lossy(&stdout).to_string())
    }
}
impl ImageSource {
    /// Q: why cant we just take the Mat and shove it in the imagesource directly
    /// A: idk api compat or whatever
    pub fn from_bytes(bytes: &[u8], (width, height): (u32, u32)) -> Result<ImageSource, OcrError> {
        let mat = Mat::new_size_with_data(
            Size_ {
                width: width.try_into()?,
                height: height.try_into()?,
            },
            bytes,
        )?;
        let png_bytes: Vec<u8> = {
            let mut buf: Vector<u8> = vec![].into();
            imencode_def(".png", &mat, &mut buf)?;
            buf.into()
        };
        Ok(ImageSource(png_bytes))
    }
}
