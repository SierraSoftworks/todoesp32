use anyhow::anyhow;
use config::HOSTNAME;
use controls::Control;
use embedded_graphics::prelude::*;
use epd_waveshare::prelude::*;
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    ipv4, netif,
    nvs::EspDefaultNvsPartition,
    sntp,
    wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use esp_idf_svc::{hal::*, wifi};
use gpio::InputPin;
use gpio::OutputPin;
use retry::retry;

mod config;
mod controls;
mod display;
mod intro;
mod markdown;
mod retry;
mod todoist;

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    let wakeup_reason = esp_idf_hal::reset::WakeupReason::get();
    println!("Wakeup reason: {:?}", wakeup_reason);

    let reset_reason = esp_idf_hal::reset::ResetReason::get();
    println!("Reset reason: {:?}", reset_reason);

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    match run() {
        Ok(()) => {
            log::info!("System exited cleanly");
            Ok(())
        }
        Err(err) => {
            log::error!("System exited with error: {:?}", err);
            Err(err)
        }
    }
}

fn run() -> anyhow::Result<()> {
    log::info!("Starting up");

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut display = display::Display::new(
        peripherals.spi2,
        peripherals.pins.gpio23.downgrade_output(),
        peripherals.pins.gpio18.downgrade_output(),
        peripherals.pins.gpio5.downgrade_output(),
        peripherals.pins.gpio17.downgrade_output(),
        peripherals.pins.gpio16.downgrade_output(),
        peripherals.pins.gpio4.downgrade_input(),
        delay::FreeRtos,
    )?;

    log::info!("Display configured");

    log::debug!("Configuring WiFi modem");

    let net_if = netif::EspNetif::new_with_conf(&netif::NetifConfiguration {
        ip_configuration: ipv4::Configuration::Client(ipv4::ClientConfiguration::DHCP(
            ipv4::DHCPClientSettings {
                hostname: Some(
                    HOSTNAME
                        .try_into()
                        .map_err(|e| anyhow!("Failed to load hostname: {:?}", e))?,
                ),
            },
        )),
        ..netif::NetifConfiguration::wifi_default_client()
    })?;

    let mut wifi = BlockingWifi::wrap(
        EspWifi::wrap_all(
            wifi::WifiDriver::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
            net_if,
            netif::EspNetif::new(netif::NetifStack::Ap)?,
        )?,
        sys_loop,
    )?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: config::WIFI_SSID
            .try_into()
            .map_err(|e| anyhow!("Failed to load WiFi SSID: {:?}.", e))?,
        password: config::WIFI_PASSWORD
            .try_into()
            .map_err(|e| anyhow!("Failed to load WiFi Password: {:?}.", e))?,
        pmf_cfg: wifi::PmfConfiguration::new_pmf_optional(),
        ..Default::default()
    }))?;
    wifi.start()?;

    log::debug!("WiFi configured, connecting...");
    wifi.connect()?;
    wifi.wait_netif_up()?;

    log::info!("Connected to WiFi network");

    log::debug!("Setting up system timezone");
    std::env::set_var("TZ", config::TIMEZONE_SPEC);

    log::debug!("Configuring system time");
    let ntp = sntp::EspSntp::new_default()?;
    while !matches!(ntp.get_sync_status(), sntp::SyncStatus::Completed) {
        log::debug!("Waiting for NTP sync...");
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
    log::info!("System time synchronized");

    let todoist = todoist::TodoistClient::new(config::TODOIST_API_KEY, config::TODOIST_FILTER)?;

    let mut header = controls::Header::new();
    let mut tasks = controls::TaskList::new(display.bounding_box().resized(
        Size::new(display.width() as u32, display.height() as u32 - 30),
        embedded_graphics::geometry::AnchorPoint::BottomLeft,
    ));
    let mut popup = controls::Popup::new(
        "Loading Error",
        "Check that your API key is correct.".into(),
    );

    tasks.set_tasks(intro::get_setup_tasks(wifi.is_connected().unwrap_or(false)));
    let mut last_update = chrono::Local::now();

    loop {
        let is_online = wifi.is_connected()?;
        header.set_date(chrono::Local::now().naive_local().date());

        if !is_online {
            header.set_last_update("Offline".to_string(), OctColor::Red);
            display.render_controls_if_dirty(OctColor::White, &mut [&mut header, &mut tasks])?;

            std::thread::sleep(std::time::Duration::from_secs(30));

            wifi.connect()?;

            continue;
        }

        match retry(|| todoist.get_tasks(), 3) {
            Ok(t) => {
                log::info!("Got {} tasks from Todoist", t.len());
                tasks.set_tasks(t);
                if tasks.is_dirty() {
                    last_update = chrono::Local::now();
                }

                header.set_last_update(
                    format!("Updated at {}", last_update.format("%H:%M")),
                    OctColor::Green,
                );
            }
            Err(_) if chrono::Local::now() - last_update < chrono::Duration::hours(1) => {
                log::info!("Failed to get tasks from Todoist, but not retrying yet");
            }
            Err(e) => {
                log::error!("Failed to get tasks from Todoist: {:?}", e);
                header.set_last_update("Update failing consistently".to_string(), OctColor::Red);
                popup.set_message(format!("{:?}", e));

                display
                    .render_controls_if_dirty(OctColor::White, &mut [&mut header, &mut popup])?;
            }
        }

        display.render_controls_if_dirty(OctColor::White, &mut [&mut header, &mut tasks])?;

        std::thread::sleep(std::time::Duration::from_secs(300));
    }
}
