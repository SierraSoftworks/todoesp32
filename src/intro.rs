use epd_waveshare::color::OctColor;

use crate::{config, controls::task_list::TaskSnapshot};

#[allow(clippy::const_is_empty)]
pub fn get_setup_tasks(wifi_connected: bool) -> Vec<TaskSnapshot> {
    let config_wifi = config::WIFI_SSID.is_empty() || config::WIFI_PASSWORD.is_empty();
    let config_todoist = config::TODOIST_API_KEY.is_empty();
    let sync_time = chrono::Local::now().timestamp() < 100000;

    vec![
        TaskSnapshot {
            title: "Configure WiFi credentials in config.rs".to_string(),
            description: Some(
                "Set the WIFI_SSID and WIFI_PASSWORD values in src/config.rs.".to_string(),
            ),
            when: if !config_wifi { "todo" } else { "done" }.to_string(),
            when_color: OctColor::Black,
            duration: None,
            marker_color: if !config_wifi {
                OctColor::Green
            } else {
                OctColor::Red
            },
        },
        TaskSnapshot {
            title: "Configure Todoist API key in config.rs".to_string(),
            description: Some("Set the TODOIST_API_KEY value in src/config.rs.".to_string()),
            when: if config_todoist { "todo" } else { "done" }.to_string(),
            when_color: OctColor::Black,
            duration: None,
            marker_color: if !config_todoist {
                OctColor::Green
            } else {
                OctColor::Red
            },
        },
        TaskSnapshot {
            title: "Connect to WiFi network".to_string(),
            description: Some(
                "Make sure that your WiFi name and password are correct.".to_string(),
            ),
            when: if wifi_connected { "todo" } else { "done" }.to_string(),
            when_color: OctColor::Black,
            duration: None,
            marker_color: if wifi_connected {
                OctColor::Green
            } else {
                OctColor::Red
            },
        },
        TaskSnapshot {
            title: "Synchronize system time".to_string(),
            description: Some(format!(
                "Wait for NTP to sync your system time correctly, it is currently {}.",
                chrono::Local::now()
            )),
            when: if !sync_time { "todo" } else { "done" }.to_string(),
            when_color: OctColor::Black,
            duration: None,
            marker_color: if !sync_time {
                OctColor::Green
            } else {
                OctColor::Red
            },
        },
        TaskSnapshot {
            title: "Synchronize Todoist tasks".to_string(),
            description: Some(
                "Make sure that your Todoist API key is correctly configured.".to_string(),
            ),
            when: "todo".to_string(),
            when_color: OctColor::Black,
            duration: None,
            marker_color: OctColor::Red,
        },
    ]
}
