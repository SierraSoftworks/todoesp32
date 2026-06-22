//! The title bar showing the current date and last-update status.

use alloc::string::String;

use embedded_graphics::geometry::AnchorPoint;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle};
use epd_waveshare::color::OctColor;
use u8g2_fonts::{FontRenderer, fonts, types};

use super::Control;
use crate::display::DisplayBuffer;

pub struct Header {
    date: Option<chrono::NaiveDate>,
    last_update: Option<String>,
    last_update_color: OctColor,
    dirty: bool,
}

#[allow(dead_code)]
impl Header {
    pub fn new() -> Self {
        Self {
            date: None,
            last_update: None,
            last_update_color: OctColor::Black,
            dirty: true,
        }
    }

    pub fn set_date(&mut self, date: chrono::NaiveDate) -> &mut Self {
        self.dirty = self.dirty || self.date != Some(date);
        self.date = Some(date);
        self
    }

    pub fn set_last_update(&mut self, message: String, color: OctColor) -> &mut Self {
        self.dirty = self.dirty
            || self.last_update.as_ref() != Some(&message)
            || self.last_update_color != color;
        self.last_update = Some(message);
        self.last_update_color = color;
        self
    }
}

impl Default for Header {
    fn default() -> Self {
        Self::new()
    }
}

impl Control for Header {
    fn render(&self, display: &mut DisplayBuffer<'_>) {
        let header_box = display
            .bounding_box()
            .resized(Size::new(display.width() as u32, 30), AnchorPoint::TopLeft);

        Line::new(
            header_box.anchor_point(AnchorPoint::BottomLeft),
            header_box.anchor_point(AnchorPoint::BottomRight),
        )
        .into_styled(PrimitiveStyle::with_stroke(OctColor::Black, 1))
        .draw(display)
        .ok();

        FontRenderer::new::<fonts::u8g2_font_helvB14_te>()
            .render_aligned(
                "Todoist",
                header_box.anchor_point(AnchorPoint::CenterLeft) + Point::new(10, 0),
                types::VerticalPosition::Center,
                types::HorizontalAlignment::Left,
                types::FontColor::Transparent(OctColor::Red),
                display,
            )
            .ok();

        if let Some(time) = self.date {
            FontRenderer::new::<fonts::u8g2_font_helvB14_te>()
                .render_aligned(
                    format_args!("{}", time.format("%a %e %B")),
                    header_box.center(),
                    types::VerticalPosition::Center,
                    types::HorizontalAlignment::Center,
                    types::FontColor::Transparent(OctColor::Black),
                    display,
                )
                .ok();
        }

        if let Some(last_update) = self.last_update.as_ref() {
            FontRenderer::new::<fonts::u8g2_font_unifont_tf>()
                .render_aligned(
                    last_update.as_str(),
                    header_box.anchor_point(AnchorPoint::CenterRight) + Point::new(-10, 0),
                    types::VerticalPosition::Center,
                    types::HorizontalAlignment::Right,
                    types::FontColor::Transparent(self.last_update_color),
                    display,
                )
                .ok();
        }
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn clear_dirty(&mut self) {
        self.dirty = false;
    }
}
