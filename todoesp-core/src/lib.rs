//! Portable application logic for the TodoESP firmware.
//!
//! This crate deliberately avoids any hardware (`esp-hal`/`embassy`) or
//! networking dependencies so that the parsing, sorting and formatting logic
//! can be unit-tested on the host with `cargo test`. The firmware crate depends
//! on this crate and provides the hardware glue (WiFi, TLS, SNTP and the
//! e-paper display).
//!
//! The crate is `no_std` when built for the firmware target and `std` when
//! built under `cargo test` (so the standard test harness is available).
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod colour;
pub mod hash;
pub mod markdown;
pub mod snapshot;
pub mod task;
pub mod time;

pub use colour::Colour;
pub use hash::{fingerprint_status, fingerprint_tasks};
pub use snapshot::{get_setup_tasks, SetupState, TaskSnapshot};
pub use task::{
    parse_tasks, FromJson, ParseError, Task, TaskDue, TaskDueState, TaskDuration, TaskStreamParser,
};
pub use time::{local_from_unix, offset_from_seconds};
