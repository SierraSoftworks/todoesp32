#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use chrono::{DateTime, FixedOffset, Timelike};
use embassy_executor::Spawner;
use embassy_net::{Config, StackResources};
use embassy_time::{Duration, Instant, with_timeout};
use embedded_graphics::geometry::{AnchorPoint, Size};
use epd_waveshare::color::OctColor;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::rng::Rng;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::rtc_cntl::sleep::{RtcSleepConfig, TimerWakeupSource};
use esp_hal::timer::timg::TimerGroup;
use log::{error, info, warn};

use todoesp_core::{
    SetupState, TaskSnapshot, fingerprint_status, fingerprint_tasks, get_setup_tasks,
    local_from_unix, offset_from_seconds,
};

use todoesp32_firmware::controls::{Header, TaskList};
use todoesp32_firmware::display::EpdDisplay;
use todoesp32_firmware::retry::retry;
use todoesp32_firmware::todoist::{ClientState, TodoistClient};
use todoesp32_firmware::{config, net};

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

/// Allocate a value with a `'static` lifetime in a `StaticCell`.
macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        STATIC_CELL.init($val)
    }};
}

/// The TLS read buffer must be able to hold a full TLS record (~16 KiB).
const TLS_READ_SIZE: usize = 16_640;
/// Outgoing TLS records (the HTTP GET request) are small, so this can be tiny.
const TLS_WRITE_SIZE: usize = 4_096;
/// Buffer for the HTTP response status line and headers (the body is streamed).
const HTTP_RX_SIZE: usize = 8_192;

/// How long the device deep-sleeps between successful refreshes.
const REFRESH_INTERVAL: Duration = Duration::from_secs(300);
/// Shorter deep-sleep used to retry after a connectivity or fetch failure.
const RETRY_INTERVAL: Duration = Duration::from_secs(60);
/// How long to wait for the WiFi link (association + DHCP) before giving up.
const WIFI_TIMEOUT: Duration = Duration::from_secs(30);
/// How long to wait for a single NTP time sync attempt before giving up.
const SNTP_TIMEOUT: Duration = Duration::from_secs(15);
/// How many additional NTP sync attempts to make after the first failure.
const SNTP_RETRIES: usize = 3;

/// Marks [`DISPLAY_FINGERPRINT`] as valid. RTC memory holds undefined contents
/// after a cold boot, so the stored fingerprint is only trusted when the
/// companion marker matches this value.
const FINGERPRINT_MAGIC: u32 = 0x7d0e_5f01;

/// Fingerprint of whatever is currently shown on the panel, kept in RTC fast
/// memory so it survives deep sleep (note: [`enter_deep_sleep`] must keep that
/// memory domain powered). We only refresh the slow, power-hungry e-paper when
/// the content we want to show differs from this value.
#[esp_hal::ram(unstable(rtc_fast, persistent))]
static mut DISPLAY_FINGERPRINT: u64 = 0;
/// Validity marker for [`DISPLAY_FINGERPRINT`]; see [`FINGERPRINT_MAGIC`].
#[esp_hal::ram(unstable(rtc_fast, persistent))]
static mut DISPLAY_FINGERPRINT_VALID: u32 = 0;

/// The kind of problem that prevented a normal refresh. Each renders a distinct
/// status screen and is fingerprinted, so a persistent failure is only drawn
/// once rather than on every retry.
#[derive(Clone, Copy, Debug)]
enum Failure {
    /// Could not associate with WiFi / obtain an IP address.
    Wifi = 0,
    /// Connected, but could not synchronise the clock over NTP.
    Time = 1,
    /// Connected with a synced clock, but could not fetch tasks from Todoist.
    Fetch = 2,
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // The reclaimed-DRAM heap (`dram2_seg`) is separate from the `.bss`/stack
    // region (`dram_seg`); the large network buffers live here at runtime.
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98_768);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);
    info!("Embassy + esp-rtos initialised");

    // The fingerprint of what is currently on the panel, if it survived from a
    // previous cycle. `None` after a cold boot (RTC memory is uninitialised) or
    // if the fingerprint could not be retained.
    let shown = load_fingerprint();

    let mut rng = Rng::new();

    let mut display = match EpdDisplay::new(
        peripherals.SPI2,
        peripherals.GPIO18,
        peripherals.GPIO23,
        peripherals.GPIO5,
        peripherals.GPIO17,
        peripherals.GPIO16,
        peripherals.GPIO4,
    ) {
        Ok(display) => display,
        Err(e) => {
            error!("Failed to initialise the display: {e:?}");
            reboot();
        }
    };
    info!("Display configured");

    let offset = offset_from_seconds(config::UTC_OFFSET_SECONDS);
    let mut header = Header::new();
    let mut tasks = TaskList::new(display.bounding_box().resized(
        Size::new(display.width() as u32, display.height() as u32 - 30),
        AnchorPoint::BottomLeft,
    ));

    // Run a single refresh cycle, render only if the result changed, then sleep.
    // Deep sleep resets the chip, so the next wake starts this function over.
    let (fingerprint, sleep_for) = match run_refresh(spawner, &mut rng, peripherals.WIFI, offset)
        .await
    {
        Ok((now, snapshots)) => {
            let date = now.date_naive();
            let fingerprint = fingerprint_tasks(date, &snapshots);
            if shown == Some(fingerprint) {
                info!("Tasks unchanged (fingerprint {fingerprint:#018x}); leaving the panel as-is");
            } else {
                info!(
                    "Task content changed (was {shown:?}, now {fingerprint:#018x}); refreshing the panel"
                );
                header.set_date(date);
                header.set_last_update(
                    alloc::format!("Updated {:02}:{:02}", now.hour(), now.minute()),
                    OctColor::Green,
                );
                tasks.set_tasks(snapshots);
                if let Err(e) = display
                    .render_controls_if_dirty(OctColor::White, &mut [&mut header, &mut tasks])
                {
                    error!("Failed to render task list: {e:?}");
                }
            }
            (fingerprint, REFRESH_INTERVAL)
        }
        Err(failure) => {
            let fingerprint = fingerprint_status(failure as u8);
            if shown == Some(fingerprint) {
                warn!("Refresh still failing ({failure:?}); status screen already shown");
            } else {
                warn!("Refresh failed ({failure:?}); showing the status screen");
                render_status(&mut display, &mut header, &mut tasks, failure, offset);
            }
            (fingerprint, RETRY_INTERVAL)
        }
    };

    store_fingerprint(fingerprint);

    // Power down the panel controller, then deep-sleep the MCU. The e-paper
    // keeps its image with no power, so the display stays visible until the
    // next refresh actually changes something.
    display.sleep();
    enter_deep_sleep(sleep_for, peripherals.LPWR);
}

/// Connect to WiFi, synchronise the clock and fetch the current tasks.
///
/// On success returns the snapshots to display and the time they were fetched;
/// otherwise returns the [`Failure`] that stopped us. Everything it allocates is
/// leaked for the cycle and reclaimed by the deep-sleep reset.
async fn run_refresh(
    spawner: Spawner,
    rng: &mut Rng,
    wifi: esp_hal::peripherals::WIFI<'static>,
    offset: FixedOffset,
) -> Result<(DateTime<FixedOffset>, alloc::vec::Vec<TaskSnapshot>), Failure> {
    let (controller, interfaces) = esp_radio::wifi::new(wifi, Default::default()).map_err(|e| {
        error!("Failed to initialise WiFi: {e:?}");
        Failure::Wifi
    })?;

    let net_seed = seed(rng);
    let resources = mk_static!(StackResources<4>, StackResources::new());
    let (stack, runner) = embassy_net::new(
        interfaces.station,
        Config::dhcpv4(Default::default()),
        resources,
        net_seed,
    );

    spawner.spawn(
        net::connection(controller, config::WIFI_SSID, config::WIFI_PASSWORD)
            .expect("failed to create the WiFi connection task"),
    );
    spawner.spawn(net::net(runner).expect("failed to create the network task"));

    info!("Waiting for the network link...");
    if with_timeout(WIFI_TIMEOUT, stack.wait_config_up())
        .await
        .is_err()
    {
        warn!("Timed out waiting for WiFi/DHCP");
        return Err(Failure::Wifi);
    }
    if let Some(cfg) = stack.config_v4() {
        info!("Got IP address: {}", cfg.address);
    }

    let boot = Instant::now();
    let base_unix = retry(
        || async {
            match with_timeout(
                SNTP_TIMEOUT,
                todoesp32_firmware::sntp::sync_unix_time(stack, config::NTP_SERVER),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => Err(todoesp32_firmware::sntp::SntpError::Timeout),
            }
        },
        SNTP_RETRIES,
    )
    .await
    .map_err(|e| {
        warn!("Failed to synchronise time over NTP: {e:?}");
        Failure::Time
    })?;
    info!("System time synchronised via NTP");

    // Heap-leaked for this cycle; the deep-sleep reset reclaims everything.
    let tcp_state: &'static ClientState =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(ClientState::new()));
    let todoist = TodoistClient::new(config::TODOIST_API_KEY, config::TODOIST_FILTER, tcp_state);
    let tls_read: &'static mut [u8] = alloc::vec![0u8; TLS_READ_SIZE].leak();
    let tls_write: &'static mut [u8] = alloc::vec![0u8; TLS_WRITE_SIZE].leak();
    let rx_buf: &'static mut [u8] = alloc::vec![0u8; HTTP_RX_SIZE].leak();

    let now_unix = base_unix + Instant::now().duration_since(boot).as_secs() as i64;
    let now = local_from_unix(now_unix, offset).ok_or(Failure::Time)?;

    let mut fetched = None;
    for attempt in 0..3 {
        let request_seed = seed(rng);
        match todoist
            .get_tasks(
                stack,
                request_seed,
                &mut tls_read[..],
                &mut tls_write[..],
                &mut rx_buf[..],
            )
            .await
        {
            Ok(tasks) => {
                fetched = Some(tasks);
                break;
            }
            Err(e) => warn!("Failed to fetch tasks (attempt {}): {e:?}", attempt + 1),
        }
    }
    let fetched = fetched.ok_or(Failure::Fetch)?;
    info!("Fetched {} tasks from Todoist", fetched.len());

    let snapshots = fetched.into_iter().map(|t| t.into_snapshot(now)).collect();
    Ok((now, snapshots))
}

/// Render the status checklist describing why a refresh failed.
fn render_status(
    display: &mut EpdDisplay,
    header: &mut Header,
    tasks: &mut TaskList,
    failure: Failure,
    offset: FixedOffset,
) {
    let now = local_from_unix(0, offset).unwrap_or_default();
    let state = SetupState {
        wifi_configured: !config::WIFI_SSID.is_empty(),
        todoist_configured: !config::TODOIST_API_KEY.is_empty(),
        wifi_connected: !matches!(failure, Failure::Wifi),
        time_synced: matches!(failure, Failure::Fetch),
    };
    tasks.set_tasks(get_setup_tasks(now, state));

    let message = match failure {
        Failure::Wifi => "WiFi unavailable",
        Failure::Time => "Clock sync failed",
        Failure::Fetch => "Todoist unreachable",
    };
    header.set_last_update(message.into(), OctColor::Red);

    if let Err(e) = display.render_controls_if_dirty(OctColor::White, &mut [header, tasks]) {
        error!("Failed to render the status screen: {e:?}");
    }
}

/// Read the panel fingerprint persisted across deep sleep, or `None` if RTC
/// memory does not currently hold a valid value (e.g. after a cold boot).
fn load_fingerprint() -> Option<u64> {
    // SAFETY: single-threaded access to RTC-persistent memory.
    unsafe {
        if (&raw const DISPLAY_FINGERPRINT_VALID).read() == FINGERPRINT_MAGIC {
            Some((&raw const DISPLAY_FINGERPRINT).read())
        } else {
            None
        }
    }
}

/// Persist the panel fingerprint (and its validity marker) across deep sleep.
fn store_fingerprint(value: u64) {
    // SAFETY: single-threaded access to RTC-persistent memory.
    unsafe {
        (&raw mut DISPLAY_FINGERPRINT).write(value);
        (&raw mut DISPLAY_FINGERPRINT_VALID).write(FINGERPRINT_MAGIC);
    }
}

/// Enter timer-wake deep sleep. The chip resets on wake and `main` runs again.
///
/// Unlike [`Rtc::sleep_deep`], this keeps the RTC fast-memory domain powered so
/// the persistent [`DISPLAY_FINGERPRINT`] actually survives the sleep — the
/// default deep-sleep config powers that memory down, which would erase the
/// fingerprint and force a full refresh on every wake.
fn enter_deep_sleep(duration: Duration, lpwr: esp_hal::peripherals::LPWR<'static>) -> ! {
    info!("Entering deep sleep for {} s", duration.as_secs());
    let mut config = RtcSleepConfig::deep();
    config.set_rtc_fastmem_pd_en(false);

    let mut rtc = Rtc::new(lpwr);
    let wake = TimerWakeupSource::new(core::time::Duration::from_secs(duration.as_secs()));
    rtc.sleep(&config, &[&wake]);
    unreachable!("deep sleep resets the chip and never returns");
}

/// Produce a 64-bit seed from the hardware RNG.
fn seed(rng: &mut Rng) -> u64 {
    ((rng.random() as u64) << 32) | rng.random() as u64
}

/// Log and reboot after an unrecoverable initialisation failure.
fn reboot() -> ! {
    error!("Rebooting in 5 seconds...");
    let delay = esp_hal::delay::Delay::new();
    delay.delay_millis(5_000);
    esp_hal::system::software_reset()
}

/// esp-backtrace `custom-halt` hook: after a panic backtrace has been printed,
/// pause briefly (so the log flushes and we don't spin in a tight reset loop)
/// then reset the device so it can recover instead of hanging forever.
#[unsafe(no_mangle)]
fn custom_halt() -> ! {
    let delay = esp_hal::delay::Delay::new();
    delay.delay_millis(3_000);
    esp_hal::system::software_reset()
}
