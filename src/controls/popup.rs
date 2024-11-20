use embedded_graphics::prelude::*;
use epd_waveshare::color::OctColor;
use u8g2_fonts::{fonts, types::*, FontRenderer};

use crate::display::DisplayBuffer;

use super::Control;

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
    fn render(&self, display: &mut DisplayBuffer) -> anyhow::Result<()> {
        let title_font = FontRenderer::new::<fonts::u8g2_font_inb27_mf>();

        title_font
            .render_aligned(
                self.title,
                display.bounding_box().center() + Point::new(0, -20),
                VerticalPosition::Baseline,
                HorizontalAlignment::Center,
                FontColor::Transparent(self.title_color),
                display,
            )
            .map_err(|e| anyhow::anyhow!("Failed to render message text: {}", e))?;

        let message_font = FontRenderer::new::<fonts::u8g2_font_inb16_mf>();
        message_font
            .render_aligned(
                self.message.as_str(),
                display.bounding_box().center() + Point::new(0, 20),
                VerticalPosition::Top,
                HorizontalAlignment::Center,
                FontColor::Transparent(self.message_color),
                display,
            )
            .map_err(|e| anyhow::anyhow!("Failed to render message text: {}", e))?;

        Ok(())
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn clear_dirty(&mut self) {
        self.dirty = false;
    }
}
