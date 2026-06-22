//! A centred, full-screen status/error message.

use alloc::string::String;

use embedded_graphics::prelude::*;
use epd_waveshare::color::OctColor;
use u8g2_fonts::{FontRenderer, fonts, types::*};

use super::Control;
use crate::display::DisplayBuffer;

pub struct Popup {
    pub title: &'static str,
    pub message: String,

    pub title_color: OctColor,
    pub message_color: OctColor,

    dirty: bool,
}

#[allow(dead_code)]
impl Popup {
    pub fn new(title: &'static str, message: String) -> Self {
        Self {
            title,
            message,
            title_color: OctColor::Black,
            message_color: OctColor::Black,
            dirty: true,
        }
    }

    pub fn set_title(&mut self, title: &'static str) -> &mut Self {
        self.dirty = self.dirty || self.title != title;
        self.title = title;
        self
    }

    pub fn set_message(&mut self, message: String) -> &mut Self {
        self.dirty = self.dirty || self.message != message;
        self.message = message;
        self
    }

    pub fn set_title_color(&mut self, color: OctColor) -> &mut Self {
        self.dirty = self.dirty || self.title_color != color;
        self.title_color = color;
        self
    }

    pub fn set_message_color(&mut self, color: OctColor) -> &mut Self {
        self.dirty = self.dirty || self.message_color != color;
        self.message_color = color;
        self
    }
}

impl Control for Popup {
    fn render(&self, display: &mut DisplayBuffer<'_>) {
        FontRenderer::new::<fonts::u8g2_font_inb27_mf>()
            .render_aligned(
                self.title,
                display.bounding_box().center() + Point::new(0, -20),
                VerticalPosition::Baseline,
                HorizontalAlignment::Center,
                FontColor::Transparent(self.title_color),
                display,
            )
            .ok();

        FontRenderer::new::<fonts::u8g2_font_inb16_mf>()
            .render_aligned(
                self.message.as_str(),
                display.bounding_box().center() + Point::new(0, 20),
                VerticalPosition::Top,
                HorizontalAlignment::Center,
                FontColor::Transparent(self.message_color),
                display,
            )
            .ok();
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn clear_dirty(&mut self) {
        self.dirty = false;
    }
}
