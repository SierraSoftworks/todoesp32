use std::cmp::Ordering;
use anyhow::anyhow;

use embedded_graphics::{geometry::*, primitives::*};
use epd_waveshare::color::OctColor;
use u8g2_fonts::FontRenderer;

use crate::display::DisplayBuffer;

use super::Control;

pub struct TaskList {
    bounding_box: Rectangle,
    tasks: Vec<Task>,
    count: usize,
    dirty: bool,
}

impl TaskList {
    pub fn new(bounding_box: Rectangle) -> Self {
        Self {
            bounding_box,
            dirty: true,
            count: 0,
            tasks: vec![],
        }
    }

    pub fn set_tasks(&mut self, mut tasks: Vec<Task>) -> &mut Self {
        let count = tasks.len();
        tasks.sort();
        tasks.truncate(12);
        self.dirty = self.dirty || self.count != count || self.tasks.len() != tasks.len() || self.tasks.iter().zip(tasks.iter()).any(|(a, b)| a != b);

        self.tasks = tasks;
        self.count = count;
        self
    }
}

impl Control for TaskList {
    fn render(&self, display: &mut DisplayBuffer) -> anyhow::Result<()> {
        // Draw the calendar plumb-line
        Line::new(self.bounding_box.top_left + Point::new(55, 0), self.bounding_box.anchor_point(AnchorPoint::BottomLeft) + Point::new(55, 0))
            .draw_styled(
                &PrimitiveStyleBuilder::new()
                    .stroke_width(1)
                    .stroke_color(OctColor::Black)
                    .build(),
                display)?;

        let margin_box = self.bounding_box.resized(Size::new(self.bounding_box.size.width - 10, self.bounding_box.size.height - 10), AnchorPoint::Center);

        const TASK_HEIGHT: u32 = 40;
        const CIRCLE_DIAMETER: i32 = 15;

        const TITLE_FONT_HEIGHT: i32 = 12;
        let title_font = FontRenderer::new::<u8g2_fonts::fonts::u8g2_font_helvR12_tf>();

        const INFO_FONT_HEIGHT: i32 = 9;
        let info_font = FontRenderer::new::<u8g2_fonts::fonts::u8g2_font_unifont_tf>();
        
        let mut remaining = self.count;
        for (i, task) in self.tasks.iter().enumerate() {
            let task_box = Rectangle::new(
                margin_box.anchor_point(AnchorPoint::TopLeft) + Point::new(0, i as i32 * TASK_HEIGHT as i32),
                Size::new(margin_box.size.width, TASK_HEIGHT),
            );

            // Don't render outside the margin box
            if task_box.anchor_point(AnchorPoint::BottomRight).y > margin_box.anchor_point(AnchorPoint::BottomRight).y {
                break;
            }

            remaining -= 1;

            // Draw the task timeline marker
            Circle::new(
                task_box.top_left + Point::new(50 - CIRCLE_DIAMETER/2, 0),
                CIRCLE_DIAMETER as u32,
            ).draw_styled(
                &PrimitiveStyleBuilder::new()
                    .fill_color(if task.completed { OctColor::Green } else { task.color })
                    .stroke_width(1)
                    .stroke_color(OctColor::Black)
                    .build(),
                display)?;

            // Draw the task title
            title_font.render_aligned(
                task.title.as_str(),
                task_box.anchor_point(AnchorPoint::TopLeft) + Point::new(50 + CIRCLE_DIAMETER/2 + 5, 0),
                u8g2_fonts::types::VerticalPosition::Top,
                u8g2_fonts::types::HorizontalAlignment::Left,
                u8g2_fonts::types::FontColor::Transparent(OctColor::Black),
                display,
            ).map_err(|_| anyhow!("Unable to render task title"))?;

            // Render the additional information text
            info_font.render_aligned(
                task.description.as_str(),
                task_box.anchor_point(AnchorPoint::TopLeft) + Point::new(50 + CIRCLE_DIAMETER/2 + 5, TITLE_FONT_HEIGHT + 5),
                u8g2_fonts::types::VerticalPosition::Top,
                u8g2_fonts::types::HorizontalAlignment::Left,
                u8g2_fonts::types::FontColor::Transparent(OctColor::Blue),
                display,
            ).map_err(|_| anyhow!("Unable to render additional information text"))?;

            // Draw the state marker (done/todo/past/time)
            if task.completed {
                info_font.render_aligned(
                    "done",
                    task_box.anchor_point(AnchorPoint::TopLeft) + Point::new(50 - CIRCLE_DIAMETER/2 - 5, (TITLE_FONT_HEIGHT - INFO_FONT_HEIGHT) / 2),
                    u8g2_fonts::types::VerticalPosition::Top,
                    u8g2_fonts::types::HorizontalAlignment::Right,
                    u8g2_fonts::types::FontColor::Transparent(OctColor::Green),
                    display,
                ).map_err(|_| anyhow!("Unable to render task done marker"))?;
            } else {
                info_font.render_aligned(
                    format_args!("{}", task.when),
                    task_box.anchor_point(AnchorPoint::TopLeft) + Point::new(50 - CIRCLE_DIAMETER/2 - 5, (TITLE_FONT_HEIGHT - INFO_FONT_HEIGHT) / 2),
                    u8g2_fonts::types::VerticalPosition::Top,
                    u8g2_fonts::types::HorizontalAlignment::Right,
                    u8g2_fonts::types::FontColor::Transparent(if task.when.is_past() { OctColor::Red } else { OctColor::Black }),
                    display,
                ).map_err(|_| anyhow!("Unable to render task time"))?;
            }

            if let Some(duration) = task.duration {
                FontRenderer::new::<u8g2_fonts::fonts::u8g2_font_unifont_tf>().render_aligned(
                    format_args!("{}m", duration.num_minutes()),
                    task_box.anchor_point(AnchorPoint::TopLeft) + Point::new(50 - CIRCLE_DIAMETER/2 - 5, TITLE_FONT_HEIGHT + 5),
                    u8g2_fonts::types::VerticalPosition::Top,
                    u8g2_fonts::types::HorizontalAlignment::Right,
                    u8g2_fonts::types::FontColor::Transparent(OctColor::Blue),
                    display,
                ).map_err(|_| anyhow!("Unable to render task duration"))?;
            }
        }

        if remaining > 0 {
            // Draw "+ N more tasks..." message
            info_font.render_aligned(
                format_args!("+ {} more...", remaining),
                margin_box.anchor_point(AnchorPoint::BottomCenter) + Point::new(0, -5),
                u8g2_fonts::types::VerticalPosition::Bottom,
                u8g2_fonts::types::HorizontalAlignment::Center,
                u8g2_fonts::types::FontColor::Transparent(OctColor::Black),
                display,
            ).map_err(|_| anyhow!("Unable to render more tasks message"))?;
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

pub struct Task {
    pub title: String,
    pub description: String,
    pub priority: u8,
    pub order: u32,
    pub when: TaskSchedule,
    pub color: OctColor,
    pub duration: Option<chrono::Duration>,
    pub completed: bool,
}

impl Eq for Task {}

impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut ordering = self.when.partial_cmp(&other.when).unwrap_or(Ordering::Equal);

        if ordering == Ordering::Equal {
            ordering = self.priority.cmp(&other.priority);
        }

        if ordering == Ordering::Equal {
            ordering = self.order.cmp(&other.order);
        }

        if ordering == Ordering::Equal {
            ordering = self.title.cmp(&other.title)
        }
        
        ordering
    }
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title && self.when == other.when
    }
}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Default, Clone, Copy, PartialEq)]
pub enum TaskSchedule {
    #[default]
    None,
    Date(chrono::NaiveDate),
    Time(chrono::NaiveDateTime),
}

impl TaskSchedule {
    pub fn is_past(&self) -> bool {
        match self {
            TaskSchedule::None => false,
            TaskSchedule::Date(date) => date < &chrono::Local::now().date_naive(),
            TaskSchedule::Time(time) => time < &chrono::Local::now().naive_local(),
        }
    }
}

impl From<chrono::NaiveDate> for TaskSchedule {
    fn from(date: chrono::NaiveDate) -> Self {
        TaskSchedule::Date(date)
    }
}

impl From<chrono::NaiveDateTime> for TaskSchedule {
    fn from(time: chrono::NaiveDateTime) -> Self {
        TaskSchedule::Time(time)
    }
}

impl<Tz> From<chrono::DateTime<Tz>> for TaskSchedule
where
    Tz: chrono::TimeZone,
{
    fn from(time: chrono::DateTime<Tz>) -> Self {
        TaskSchedule::Time(time.naive_local())
    }
}

impl std::fmt::Display for TaskSchedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskSchedule::None => write!(f, "todo"),
            TaskSchedule::Date(date) if date == &chrono::Local::now().date_naive() => write!(f, "today"),
            TaskSchedule::Date(_date) => write!(f, "past"),
            TaskSchedule::Time(time) if time.date() == chrono::Local::now().date_naive() => write!(f, "{}", time.format("%H:%M")),
            TaskSchedule::Time(_time) => write!(f, "past"),
        }
    }
}

impl PartialOrd for TaskSchedule {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (TaskSchedule::None, TaskSchedule::None) => Some(Ordering::Equal),
            (TaskSchedule::None, _) => Some(Ordering::Greater),
            (_, TaskSchedule::None) => Some(Ordering::Less),
            (TaskSchedule::Date(a), TaskSchedule::Date(b)) => Some(a.cmp(b)),
            (TaskSchedule::Date(_), TaskSchedule::Time(_)) => Some(Ordering::Greater),
            (TaskSchedule::Time(_), TaskSchedule::Date(_)) => Some(Ordering::Less),
            (TaskSchedule::Time(a), TaskSchedule::Time(b)) => Some(a.cmp(b)),
        }
    }
}
