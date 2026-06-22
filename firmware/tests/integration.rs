//! On-device smoke tests for the TodoESP firmware.
//!
//! The portable application logic (task parsing, time conversion, snapshot
//! rendering data) is comprehensively unit-tested on the host in the
//! `todoesp-core` crate — run those with `cargo test -p todoesp-core`.
//!
//! These tests instead verify that the *same* logic links and runs correctly in
//! a real `no_std` xtensa environment on the ESP32. They use the
//! [`embedded-test`](https://crates.io/crates/embedded-test) harness together
//! with [`probe-rs`](https://probe.rs/), which talks to the chip over JTAG.
//!
//! ⚠️ The classic ESP32 has **no built-in USB-JTAG** (unlike the S3/C3/C6), so an
//! external JTAG probe is required to run these. With a probe connected:
//!
//! ```sh
//! cargo install probe-rs-tools
//! cd firmware
//! cargo test --config 'target.xtensa-esp32-none-elf.runner="probe-rs run --chip esp32"'
//! ```
//!
//! The default `cargo run`/`espflash` (UART) flashing workflow is unaffected,
//! because the probe-rs runner is only supplied on the `cargo test` invocation.

#![no_std]
#![no_main]

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    use chrono::Datelike;
    use todoesp_core::{local_from_unix, offset_from_seconds, parse_tasks};

    const SAMPLE: &[u8] = br#"{
        "results": [
            {"id":"b","priority":1,"child_order":3,"content":"**Buy** milk","description":"from the *store*","due":{"date":"2021-01-01T09:00:00Z"},"checked":false,"duration":{"amount":30,"unit":"minute"}},
            {"id":"a","priority":4,"child_order":1,"content":"Call mum","description":"","due":{"date":"2021-01-01"},"checked":false,"duration":null},
            {"id":"c","priority":2,"child_order":2,"content":"No due date","description":"","due":null,"checked":false,"duration":null}
        ]
    }"#;

    /// Initialise the chip and a heap so `alloc`-based logic works on-device.
    #[init]
    fn init() {
        let _ = esp_hal::init(esp_hal::Config::default());
        esp_alloc::heap_allocator!(size: 64 * 1024);
    }

    /// JSON parsing, sorting and the heap allocator all work on real hardware.
    #[test]
    fn parses_and_sorts_tasks_on_device() {
        let tasks = parse_tasks(SAMPLE).expect("valid json");
        assert_eq!(tasks.len(), 3);

        // The all-day task ("Call mum") sorts before the timed task on the same
        // day, both before the task with no due date.
        let now = local_from_unix(1_609_495_200, offset_from_seconds(0)).expect("valid time");
        let first = tasks.into_iter().next().unwrap().into_snapshot(now);
        assert_eq!(first.title.as_str(), "Call mum");
    }

    /// `chrono` time conversion produces correct results on the target.
    #[test]
    fn converts_unix_time_with_offset_on_device() {
        let offset = offset_from_seconds(3600);
        let when = local_from_unix(1_609_495_200, offset).expect("valid time");
        // 1609495200 = 2021-01-01T10:00:00+01:00
        assert_eq!(when.year(), 2021);
        assert_eq!(when.month(), 1);
        assert_eq!(when.day(), 1);
    }
}
