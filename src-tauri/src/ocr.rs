use crate::errors::OcrError;
use opencv::{core::Size_, prelude::*};
use std::path::PathBuf;

#[cfg(not(feature = "compile-tesseract"))]
pub use tesseract_system::*;

#[cfg(feature = "compile-tesseract")]
pub use tesseract_lib::*;

#[cfg(not(feature = "compile-tesseract"))]
mod tesseract_system {
    use super::*;
    use opencv::{core::Vector, imgcodecs::imencode_def};
    use std::{
        io::Write,
        process::{Command, Output, Stdio},
    };
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
        pub fn from_bytes(
            bytes: &[u8],
            (width, height): (u32, u32),
        ) -> Result<ImageSource, OcrError> {
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
}

#[cfg(feature = "compile-tesseract")]
mod tesseract_lib {
    use super::*;
    use std::cell::RefCell;
    use tesseract::{PageSegMode, Tesseract, TesseractError};
    /// An `ocrs::OcrEngine`-like wrapper for tesseract, provided by ~~opencv::text::OCRTesseract~~ ~~THE
    /// FUCKING TESSERACT COMMAND I'M TIRED OF LINKING ISSUES.~~ `tesseract::Tesseract` :yay:
    pub struct OcrEngine {
        tesseract: RefCell<Option<tesseract::Tesseract>>,
    }
    pub struct ImageSource(Mat);

    impl OcrEngine {
        pub fn check_tesseract() -> Result<bool, OcrError> {
            Ok(true)
        }
        pub fn new(datapath: PathBuf) -> Result<OcrEngine, OcrError> {
            let mut tesseract =
                Tesseract::new(datapath.to_str(), Some("eng")).map_err(TesseractError::from)?;
            tesseract.set_page_seg_mode(PageSegMode::PsmSingleLine);
            Ok(OcrEngine {
                tesseract: RefCell::new(Some(tesseract)),
            })
        }
        /// Q: whats the point of this
        /// A: mraow :3c
        #[inline(always)]
        pub fn prepare_input(&self, src: ImageSource) -> Result<ImageSource, OcrError> {
            Ok(src)
        }
        pub fn get_text(&self, src: ImageSource) -> Result<String, OcrError> {
            let mut self_mut = self.tesseract.borrow_mut();
            let mut tess = self_mut.take().expect("should have tesseract instance");

            let width = src.0.cols();
            let height = src.0.rows();
            let bytes_per_pixel = src.0.channels();
            let bytes_per_line = src.0.step1_def()? as i32;

            tess = tess
                .set_frame(
                    src.0.data_bytes()?,
                    width,
                    height,
                    bytes_per_pixel,
                    bytes_per_line,
                )
                .map_err(TesseractError::from)?;

            let result = tess.get_text().map_err(TesseractError::from)?;

            _ = self_mut.insert(tess);

            Ok(result)
        }
    }

    impl ImageSource {
        /// Q: why cant we just take the Mat and shove it in the imagesource directly
        /// A: idk api compat or whatever
        pub fn from_bytes(
            bytes: &[u8],
            (width, height): (u32, u32),
        ) -> Result<ImageSource, OcrError> {
            let mat = Mat::new_size_with_data(
                Size_ {
                    width: width.try_into()?,
                    height: height.try_into()?,
                },
                bytes,
            )?;
            Ok(ImageSource(mat.clone_pointee()))
        }
    }
}
