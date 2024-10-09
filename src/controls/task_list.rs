use anyhow::anyhow;

use embedded_graphics::{geometry::*, primitives::*};
use epd_waveshare::color::OctColor;
use u8g2_fonts::FontRenderer;

use crate::display::DisplayBuffer;

use super::Control;

pub struct TaskList {
    bounding_box: Rectangle,
    tasks: Vec<TaskSnapshot>,
    count: usize,
    dirty: bool,
}

#[derive(PartialEq)]
pub struct TaskSnapshot {
    pub marker_color: OctColor,

    pub title: String,
    pub description: Option<String>,

    pub when: String,
    pub when_color: OctColor,
    pub duration: Option<String>,
}

#[allow(dead_code)]
impl TaskList {
    pub fn new(bounding_box: Rectangle) -> Self {
        Self {
            bounding_box,
            dirty: true,
            count: 0,
            tasks: vec![],
        }
    }

    pub fn set_tasks<I, T>(&mut self, tasks: T) -> &mut Self
    where
        T: IntoIterator<Item = I>,
        I: Into<TaskSnapshot>,
    {
        let mut count = 0;
        let mut new_tasks: Vec<TaskSnapshot> = Vec::with_capacity(12);
        for task in tasks {
            count += 1;
            if new_tasks.len() < 12 {
                new_tasks.push(task.into());
            }
        }

        self.dirty = self.dirty
            || self.count != count
            || self.tasks.len() != new_tasks.len()
            || self
                .tasks
                .iter()
                .zip(new_tasks.iter())
                .any(|(a, b)| !a.eq(b));

        self.tasks = new_tasks;
        self.count = count;
        self
    }
}

impl Control for TaskList {
    fn render(&self, display: &mut DisplayBuffer) -> anyhow::Result<()> {
        // Draw the calendar plumb-line
        Line::new(
            self.bounding_box.top_left + Point::new(55, 0),
            self.bounding_box.anchor_point(AnchorPoint::BottomLeft) + Point::new(55, 0),
        )
        .draw_styled(
            &PrimitiveStyleBuilder::new()
                .stroke_width(1)
                .stroke_color(OctColor::Black)
                .build(),
            display,
        )?;

        let margin_box = self.bounding_box.resized(
            Size::new(
                self.bounding_box.size.width - 10,
                self.bounding_box.size.height - 10,
            ),
            AnchorPoint::Center,
        );

        const TASK_HEIGHT: u32 = 40;
        const CIRCLE_DIAMETER: i32 = 15;

        const TITLE_FONT_HEIGHT: i32 = 12;
        let title_font = FontRenderer::new::<u8g2_fonts::fonts::u8g2_font_helvR12_tf>();

        const INFO_FONT_HEIGHT: i32 = 9;
        let info_font = FontRenderer::new::<u8g2_fonts::fonts::u8g2_font_unifont_tf>();

        let mut remaining = self.count;
        for (i, task) in self.tasks.iter().enumerate() {
            let task_box = Rectangle::new(
                margin_box.anchor_point(AnchorPoint::TopLeft)
                    + Point::new(0, i as i32 * TASK_HEIGHT as i32),
                Size::new(margin_box.size.width, TASK_HEIGHT),
            );

            // Don't render outside the margin box
            if task_box.anchor_point(AnchorPoint::BottomRight).y
                > margin_box.anchor_point(AnchorPoint::BottomRight).y
            {
                break;
            }

            remaining -= 1;

            // Draw the task timeline marker
            Circle::new(
                task_box.top_left + Point::new(50 - CIRCLE_DIAMETER / 2, 0),
                CIRCLE_DIAMETER as u32,
            )
            .draw_styled(
                &PrimitiveStyleBuilder::new()
                    .fill_color(task.marker_color)
                    .stroke_width(1)
                    .stroke_color(OctColor::Black)
                    .build(),
                display,
            )?;

            // Draw the task title
            title_font
                .render_aligned(
                    task.title.as_str(),
                    task_box.anchor_point(AnchorPoint::TopLeft)
                        + Point::new(50 + CIRCLE_DIAMETER / 2 + 5, 0),
                    u8g2_fonts::types::VerticalPosition::Top,
                    u8g2_fonts::types::HorizontalAlignment::Left,
                    u8g2_fonts::types::FontColor::Transparent(OctColor::Black),
                    display,
                )
                .map_err(|_| anyhow!("Unable to render task title"))?;

            // Render the additional information text
            if let Some(description) = task.description.as_deref() {
                info_font
                    .render_aligned(
                        description,
                        task_box.anchor_point(AnchorPoint::TopLeft)
                            + Point::new(50 + CIRCLE_DIAMETER / 2 + 5, TITLE_FONT_HEIGHT + 5),
                        u8g2_fonts::types::VerticalPosition::Top,
                        u8g2_fonts::types::HorizontalAlignment::Left,
                        u8g2_fonts::types::FontColor::Transparent(OctColor::Blue),
                        display,
                    )
                    .map_err(|_| anyhow!("Unable to render additional information text"))?;
            }

            // Draw the "when" marker (done/todo/past/time)
            info_font
                .render_aligned(
                    format_args!("{}", task.when),
                    task_box.anchor_point(AnchorPoint::TopLeft)
                        + Point::new(
                            50 - CIRCLE_DIAMETER / 2 - 5,
                            (TITLE_FONT_HEIGHT - INFO_FONT_HEIGHT) / 2,
                        ),
                    u8g2_fonts::types::VerticalPosition::Top,
                    u8g2_fonts::types::HorizontalAlignment::Right,
                    u8g2_fonts::types::FontColor::Transparent(task.when_color),
                    display,
                )
                .map_err(|_| anyhow!("Unable to render task time"))?;

            if let Some(duration) = task.duration.as_deref() {
                FontRenderer::new::<u8g2_fonts::fonts::u8g2_font_unifont_tf>()
                    .render_aligned(
                        duration,
                        task_box.anchor_point(AnchorPoint::TopLeft)
                            + Point::new(50 - CIRCLE_DIAMETER / 2 - 5, TITLE_FONT_HEIGHT + 5),
                        u8g2_fonts::types::VerticalPosition::Top,
                        u8g2_fonts::types::HorizontalAlignment::Right,
                        u8g2_fonts::types::FontColor::Transparent(OctColor::Blue),
                        display,
                    )
                    .map_err(|_| anyhow!("Unable to render task duration"))?;
            }
        }

        if remaining > 0 {
            // Draw "+ N more tasks..." message
            info_font
                .render_aligned(
                    format_args!("+ {} more...", remaining),
                    margin_box.anchor_point(AnchorPoint::BottomCenter) + Point::new(0, -5),
                    u8g2_fonts::types::VerticalPosition::Bottom,
                    u8g2_fonts::types::HorizontalAlignment::Center,
                    u8g2_fonts::types::FontColor::Transparent(OctColor::Black),
                    display,
                )
                .map_err(|_| anyhow!("Unable to render more tasks message"))?;
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
