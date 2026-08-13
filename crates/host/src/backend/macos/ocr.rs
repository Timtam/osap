//! Reading text off the screen with Vision.
//!
//! The regions this is asked about are tiny and the text in them is small — a parameter
//! read-out of a few characters, sometimes a single digit. Word boxes come back in
//! capture-region coordinates; the host adds the region origin itself.

use crate::backend::OcrText;

pub fn recognize(
    _x: i32,
    _y: i32,
    _w: i32,
    _h: i32,
    _lang: Option<&str>,
) -> Result<OcrText, String> {
    Err("OCR is not implemented on macOS yet".to_string())
}
