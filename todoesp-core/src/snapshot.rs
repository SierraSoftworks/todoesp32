//! Renderer-agnostic snapshot of a task and the first-run setup checklist.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use chrono::{DateTime, FixedOffset};

use crate::colour::Colour;

/// A flattened, renderer-agnostic view of a task ready to be drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSnapshot {
    pub marker_color: Colour,

    pub title: String,
    pub description: Option<String>,

    pub when: String,
    pub when_color: Colour,
    pub duration: Option<String>,
}

/// Current configuration/connectivity state used to render the first-run
/// setup checklist.
#[derive(Debug, Clone, Copy, Default)]
pub struct SetupState {
    /// WiFi SSID and password have been configured.
    pub wifi_configured: bool,
    /// A Todoist API key has been configured.
    pub todoist_configured: bool,
    /// The device is currently connected to WiFi.
    pub wifi_connected: bool,
    /// The system clock has been synchronised via NTP.
    pub time_synced: bool,
}

/// Build the introductory setup checklist shown before any tasks are loaded.
pub fn get_setup_tasks(now: DateTime<FixedOffset>, state: SetupState) -> Vec<TaskSnapshot> {
    let config_wifi = !state.wifi_configured;
    let config_todoist = !state.todoist_configured;
    let sync_time = !state.time_synced;
    let wifi_connected = state.wifi_connected;

    vec![
        TaskSnapshot {
            title: "Configure WiFi credentials in config.rs".to_string(),
            description: Some(
                "Set the WIFI_SSID and WIFI_PASSWORD values in src/config.rs.".to_string(),
            ),
            when: if !config_wifi { "todo" } else { "done" }.to_string(),
            when_color: Colour::Black,
            duration: None,
            marker_color: if !config_wifi {
                Colour::Green
            } else {
                Colour::Red
            },
        },
        TaskSnapshot {
            title: "Configure Todoist API key in config.rs".to_string(),
            description: Some("Set the TODOIST_API_KEY value in src/config.rs.".to_string()),
            when: if config_todoist { "todo" } else { "done" }.to_string(),
            when_color: Colour::Black,
            duration: None,
            marker_color: if !config_todoist {
                Colour::Green
            } else {
                Colour::Red
            },
        },
        TaskSnapshot {
            title: "Connect to WiFi network".to_string(),
            description: Some(
                "Make sure that your WiFi name and password are correct.".to_string(),
            ),
            when: if wifi_connected { "todo" } else { "done" }.to_string(),
            when_color: Colour::Black,
            duration: None,
            marker_color: if wifi_connected {
                Colour::Green
            } else {
                Colour::Red
            },
        },
        TaskSnapshot {
            title: "Synchronize system time".to_string(),
            description: Some(format!(
                "Wait for NTP to sync your system time correctly, it is currently {}.",
                now
            )),
            when: if !sync_time { "todo" } else { "done" }.to_string(),
            when_color: Colour::Black,
            duration: None,
            marker_color: if !sync_time {
                Colour::Green
            } else {
                Colour::Red
            },
        },
        TaskSnapshot {
            title: "Synchronize Todoist tasks".to_string(),
            description: Some(
                "Make sure that your Todoist API key is correctly configured.".to_string(),
            ),
            when: "todo".to_string(),
            when_color: Colour::Black,
            duration: None,
            marker_color: Colour::Red,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::offset_from_seconds;
    use chrono::TimeZone;

    fn now() -> DateTime<FixedOffset> {
        offset_from_seconds(0)
            .timestamp_opt(1_609_459_200, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn returns_five_setup_steps() {
        let tasks = get_setup_tasks(now(), SetupState::default());
        assert_eq!(tasks.len(), 5);
    }

    #[test]
    fn time_step_includes_current_time() {
        let tasks = get_setup_tasks(now(), SetupState::default());
        let time_step = &tasks[3];
        assert!(time_step.description.as_deref().unwrap().contains("2021"));
    }
}
