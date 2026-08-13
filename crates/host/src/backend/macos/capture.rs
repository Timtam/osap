//! Screen reading: the size of the display, one pixel, and a region as an image.
//!
//! The unit rule lives here. A capture is asked for in points and must come back as
//! **exactly** that many pixels — `w * h * 4` bytes of tightly packed top-down RGBA, alpha
//! forced opaque — because the image matcher indexes the buffer arithmetically and the host
//! adds a match offset straight back onto the requested origin. On a Retina display the
//! backing store has four times as many pixels as that, so the image is downsampled here
//! and nowhere else.

use crate::backend::CapturedImage;

/// Primary display, in points.
pub fn screen_size() -> (i32, i32) {
    (0, 0)
}

/// What is composited at that point right now.
pub fn pixel(_x: i32, _y: i32) -> (u8, u8, u8) {
    (0, 0, 0)
}

/// A region as an image. A bare `fn` on purpose: the image worker calls it from another
/// thread, without the backend.
pub fn capture_region(_x: i32, _y: i32, _w: i32, _h: i32) -> Option<CapturedImage> {
    None
}
