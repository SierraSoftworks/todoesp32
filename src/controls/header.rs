use embedded_graphics::{prelude::*, primitives::*};
use epd_waveshare::color::OctColor;
use u8g2_fonts::{*, fonts, types};

use crate::display::DisplayBuffer;

use super::Control;

pub struct Header {
    date: Option<chrono::NaiveDate>,
    last_update: Option<String>,
    last_update_color: OctColor,
    dirty: bool,
}

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
        self.dirty = self.dirty || !self.last_update.as_ref().map(|u| u == &message).unwrap_or_default() || self.last_update_color != color;
        self.last_update = Some(message);
        self.last_update_color = color;
        self
    }
}

impl Control for Header {
    

    fn render(&self, display: &mut DisplayBuffer) -> anyhow::Result<()> {
        let header_box = display.bounding_box().resized(Size::new(display.width() as u32, 30), embedded_graphics::geometry::AnchorPoint::TopLeft);

        Line::new(header_box.anchor_point(embedded_graphics::geometry::AnchorPoint::BottomLeft), header_box.anchor_point(embedded_graphics::geometry::AnchorPoint::BottomRight))
            .into_styled(PrimitiveStyle::with_stroke(OctColor::Black, 1))
            .draw(display)?;

        FontRenderer::new::<fonts::u8g2_font_helvB14_te>().render_aligned(
            "Todoist",
            header_box.anchor_point(embedded_graphics::geometry::AnchorPoint::CenterLeft) + Point::new(10, 0),
            types::VerticalPosition::Center,
            types::HorizontalAlignment::Left,
            types::FontColor::Transparent(OctColor::Red),
            display,
        ).map_err(|_| anyhow::anyhow!("Unable to render title text"))?;

        if let Some(time) = self.date {
            FontRenderer::new::<fonts::u8g2_font_helvB14_te>().render_aligned(
                format_args!("{}", time.format("%a %e %B")),
                header_box.center() + Point::new(0, 0),
                types::VerticalPosition::Center,
                types::HorizontalAlignment::Center,
                types::FontColor::Transparent(OctColor::Black),
                display,
            ).map_err(|_| anyhow::anyhow!("Unable to render time text"))?;
        }

        if let Some(last_update) = self.last_update.as_ref() {
            FontRenderer::new::<fonts::u8g2_font_unifont_tf>().render_aligned(
                last_update.as_str(),
                header_box.anchor_point(embedded_graphics::geometry::AnchorPoint::CenterRight) + Point::new(-10, 0),
                types::VerticalPosition::Center,
                types::HorizontalAlignment::Right,
                types::FontColor::Transparent(self.last_update_color),
                display,
            ).map_err(|_| anyhow::anyhow!("Unable to render last update text"))?;
        }

        Ok(())
    }
    
    fn is_dirty(&self) -> bool {
        self.dirty
    }
    
    fn clear_dirty(&mut self) {
        self.dirty = false;
    }
}