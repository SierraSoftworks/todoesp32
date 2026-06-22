//! Hardware and networking glue for the TodoESP firmware.
//!
//! Portable, host-testable logic lives in the `todoesp-core` crate; this crate
//! provides the `no_std` modules that drive the e-paper display, WiFi, SNTP and
//! the Todoist HTTPS client. The binary entry point is in `src/bin/main.rs`.

#![no_std]

extern crate alloc;

pub mod config;
pub mod controls;
pub mod display;
pub mod net;
pub mod retry;
pub mod sntp;
pub mod todoist;
