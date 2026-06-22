# TodoESP

**Keep track of your Todoist tasks on an ESP32 + ePaper display.**

This project runs on an ESP32 micro-controller with a
[Waveshare 5.65" e-Paper Module (F)](https://www.waveshare.com/product/5.65inch-e-paper-module-f.htm).
When configured, it connects to your WiFi, fetches the items due on your Todoist
task list, and shows them on the display. Tasks refresh every 5 minutes and the
screen is only redrawn when something changes (avoiding unnecessary e-paper
refreshes).

The firmware is written in **`no_std` Rust** on top of
[`esp-hal`](https://github.com/esp-rs/esp-hal),
[`esp-rtos`](https://github.com/esp-rs/esp-hal) and the
[Embassy](https://embassy.dev) async runtime, following the modern
[`esp-generate`](https://github.com/esp-rs/esp-generate) baseline.

## Project layout

This is a Cargo workspace split into two crates:

| Path            | Description                                                                                                                                                                                          |
| --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `todoesp-core/` | Portable, **host-testable** application logic: Todoist parsing, time handling, markdown striping, colours and the data used to render each task. `no_std` but builds and tests on your host machine. |
| `firmware/`     | The `no_std` Xtensa ESP32 binary: e-paper driver, WiFi, SNTP, the HTTPS Todoist client and the Embassy main loop. Depends on `todoesp-core`.                                                         |

`firmware/` is intentionally **excluded** from the root workspace so its Xtensa
`.cargo/config.toml` and `esp` toolchain do not interfere with host builds and
tests of `todoesp-core`.

## BOM

- [ESP32 DevKitC](https://www.espressif.com/en/products/devkits/esp32-devkitc/overview) or equivalent ESP32 with 4MB+ of Flash
- [Waveshare 5.65" e-Paper Module (F)](https://www.waveshare.com/product/5.65inch-e-paper-module-f.htm)
- USB-C power supply (5V, 500mA)
- 3D printed case (optional, model will be published soon)

In total, the project should cost somewhere in the range of EUR70-90 depending on
where you source your parts.

### Wiring

The e-Paper module connects to the ESP32 over SPI using the following pins:

| ESP32 Pin | e-Paper Pin |
| --------- | ----------- |
| GPIO 23   | DIN         |
| GPIO 18   | CLK         |
| GPIO 5    | CS          |
| GPIO 17   | DC          |
| GPIO 16   | RST         |
| GPIO 4    | BUSY        |

**NOTE** You can change these in [`firmware/src/bin/main.rs`](firmware/src/bin/main.rs)
(passed to `EpdDisplay::new`), but be aware that not all pins are created equal
and some might not work as expected.

## Getting started

### Prerequisites

The firmware targets the Xtensa ESP32, which needs the `esp` Rust toolchain and
flashing tools:

```sh
cargo install espup espflash
espup install        # installs the `esp` toolchain + Xtensa GCC
```

The portable `todoesp-core` crate builds with any recent stable Rust toolchain.

### Configuration

The firmware reads your secrets from `firmware/src/config.rs`, which is
**git-ignored**. Copy the template and fill it in:

```sh
cp firmware/src/config.example.rs firmware/src/config.rs
$EDITOR firmware/src/config.rs
```

| Constant             | Description                                          |
| -------------------- | ---------------------------------------------------- |
| `HOSTNAME`           | Network hostname for the device.                     |
| `WIFI_SSID`          | Your WiFi network name.                              |
| `WIFI_PASSWORD`      | Your WiFi password.                                  |
| `TODOIST_API_KEY`    | Your Todoist API token.                              |
| `TODOIST_FILTER`     | A Todoist filter query (e.g. `today \| overdue`).    |
| `UTC_OFFSET_SECONDS` | Your timezone offset from UTC, in seconds.           |
| `NTP_SERVER`         | NTP server used for time sync (e.g. `pool.ntp.org`). |

### Build & flash

```sh
cd firmware
cargo run --release      # builds, flashes over USB (espflash) and opens the monitor
```

## Testing

Most of the interesting logic lives in `todoesp-core` and is covered by host
unit tests — run them with any stable toolchain:

```sh
cargo test -p todoesp-core
```

There are also **on-device smoke tests** in
[`firmware/tests/integration.rs`](firmware/tests/integration.rs) that verify the
same logic links and runs in a real `no_std` Xtensa environment. They use the
[`embedded-test`](https://crates.io/crates/embedded-test) harness with
[`probe-rs`](https://probe.rs/), which talks to the chip over **JTAG**. The
classic ESP32 has no built-in USB-JTAG (unlike the S3/C3/C6), so an external JTAG
probe is required:

```sh
cargo install probe-rs-tools
cd firmware
cargo test --config 'target.xtensa-esp32-none-elf.runner="probe-rs run --chip esp32"'
```

The normal `cargo run`/`espflash` (UART) workflow is unaffected by this — the
probe-rs runner is only supplied on the `cargo test` invocation.
