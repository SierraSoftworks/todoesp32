//! Colour alias shared between the application logic and the e-paper renderer.

/// The colour type used throughout the application.
///
/// This is the 7-colour (plus "clean") palette supported by the Waveshare
/// 5.65" (F) ACeP display.
pub type Colour = epd_waveshare::color::OctColor;
