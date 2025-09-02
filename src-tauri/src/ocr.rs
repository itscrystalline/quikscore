use opencv::{
    core::{Ptr, Size_},
    prelude::*,
    text::{OCRTesseract, OEM_DEFAULT, PSM_AUTO_OSD},
};
use std::cell::RefCell;

use crate::errors::OcrError;

/// An `ocrs::OcrEngine`-like wrapper for tesseract, provided by `opencv::text::OCRTesseract`.
pub struct OcrEngine {
    /// YAYY I LOVE INTERIOR MUTABILITY YAYYY
    inner: RefCell<OcrEngineInner>,
}
struct OcrEngineInner {
    engine: Ptr<OCRTesseract>,
}
pub struct ImageSource(Mat);

impl OcrEngineInner {
    fn new(datapath: &str) -> Result<OcrEngineInner, OcrError> {
        Ok(OcrEngineInner {
            engine: OCRTesseract::create(datapath, "eng", "", OEM_DEFAULT, PSM_AUTO_OSD)
                .map_err(OcrError::Creation)?,
        })
    }
    fn run(&mut self, prepared: ImageSource) -> Result<String, OcrError> {
        <Ptr<OCRTesseract> as OCRTesseractTrait>::run_def(&mut self.engine, &prepared.0, 95)
            .map_err(OcrError::GettingText)
    }
}
impl OcrEngine {
    pub fn new(datapath: &str) -> Result<OcrEngine, OcrError> {
        Ok(OcrEngine {
            inner: RefCell::new(OcrEngineInner::new(datapath)?),
        })
    }
    /// Q: whats the point of this
    /// A: mraow :3c
    #[inline(always)]
    pub fn prepare_input(&self, src: ImageSource) -> Result<ImageSource, OcrError> {
        Ok(src)
    }
    pub fn get_text(&self, src: ImageSource) -> Result<String, OcrError> {
        let mut borrow = self.inner.borrow_mut();
        borrow.run(src)
    }
}
impl ImageSource {
    /// Q: why cant we just take the Mat and shove it in the imagesource directly
    /// A: idk api compat or whatever
    pub fn from_bytes(bytes: &[u8], (width, height): (u32, u32)) -> ImageSource {
        ImageSource(
            Mat::new_size_with_data(
                Size_ {
                    width: width.try_into().expect("should've came from a `Mat`"),
                    height: height.try_into().expect("should've came from a `Mat`"),
                },
                bytes,
            )
            .expect("should've came from a `Mat`")
            .clone_pointee(),
        )
    }
}
