//! WiFi station connection management and the embassy-net background runner.

use embassy_net::Runner;
use embassy_time::{Duration, Timer};
use esp_radio::wifi::{
    Config as WifiConfig, Interface, WifiController,
    sta::{ScanMethod, StationConfig},
};
use log::{info, warn};

/// The embassy-net `Driver` provided by the WiFi station interface.
pub type WifiDevice = Interface<'static>;

/// Keep the WiFi station connected, reconnecting whenever the link drops.
#[embassy_executor::task]
pub async fn connection(
    mut controller: WifiController<'static>,
    ssid: &'static str,
    password: &'static str,
) {
    info!("WiFi connection task started");
    loop {
        if controller.is_connected() {
            // Wait until we lose the connection, then back off before retrying.
            controller.wait_for_disconnect_async().await.ok();
            Timer::after(Duration::from_secs(5)).await;
        }

        // Scan every channel (rather than the default fast scan, which stops at the
        // first matching AP) so the radio can compare every AP broadcasting the SSID.
        // esp-radio sorts candidates by signal strength, so this connects us to the
        // AP with the best RSSI instead of whichever one happened to answer first.
        // `failure_retry_cnt` only takes effect with an all-channel scan and lets the
        // radio fall back to the next-best AP if the strongest one refuses us.
        let client_config = WifiConfig::Station(
            StationConfig::default()
                .with_ssid(ssid)
                .with_password(password.into())
                .with_scan_method(ScanMethod::AllChannels)
                .with_failure_retry_cnt(3),
        );
        if let Err(e) = controller.set_config(&client_config) {
            warn!("Failed to set WiFi configuration: {e:?}");
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }

        info!("Connecting to WiFi network '{ssid}'...");
        match controller.connect_async().await {
            Ok(_) => info!("Connected to WiFi network"),
            Err(e) => {
                warn!("Failed to connect to WiFi: {e:?}");
                Timer::after(Duration::from_secs(5)).await;
            }
        }
    }
}

/// Drive the embassy-net stack (DHCP, ARP, socket I/O).
#[embassy_executor::task]
pub async fn net(mut runner: Runner<'static, WifiDevice>) -> ! {
    runner.run().await
}
