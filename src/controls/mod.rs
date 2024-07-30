pub mod header;
pub mod popup;
pub mod task_list;

pub use header::Header;
pub use task_list::{TaskList, TaskSnapshot};

use crate::display::DisplayBuffer;

pub trait Control {
    fn is_dirty(&self) -> bool;
    fn clear_dirty(&mut self);

    fn render(&self, display: &mut DisplayBuffer) -> anyhow::Result<()>;
}
