//! On-screen widgets that draw into the [`crate::display::DisplayBuffer`].

pub mod header;
pub mod popup;
pub mod task_list;

pub use header::Header;
pub use popup::Popup;
pub use task_list::TaskList;

use crate::display::DisplayBuffer;

/// A drawable widget that tracks whether it needs re-rendering.
///
/// Rendering is infallible: controls draw into an in-memory buffer (the panel
/// itself is only touched by [`crate::display::EpdDisplay`]). Font-fit and
/// draw-target errors are intentionally swallowed at the call site.
pub trait Control {
    fn is_dirty(&self) -> bool;
    fn clear_dirty(&mut self);

    fn render(&self, display: &mut DisplayBuffer<'_>);
}
