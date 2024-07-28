use epd_waveshare::color::OctColor;

use crate::{config, controls::task_list::{Task, TaskSchedule}};

pub fn get_setup_tasks(wifi_connected: bool) -> Vec<Task> {
    vec![
        Task {
            title: "Configure WiFi credentials in config.rs".to_string(),
            description: "Set the WIFI_SSID and WIFI_PASSWORD values in src/config.rs.".to_string(),
            priority: 1,
            order: 1,
            when: TaskSchedule::None,
            color: OctColor::Red,
            duration: None,
            #[allow(clippy::const_is_empty)]
            completed: !config::WIFI_SSID.is_empty() && !config::WIFI_PASSWORD.is_empty(),
        },
        Task {
            title: "Configure Todoist API key in config.rs".to_string(),
            description: "Set the TODOIST_API_KEY value in src/config.rs.".to_string(),
            priority: 1,
            order: 2,
            when: TaskSchedule::None,
            color: OctColor::Red,
            duration: None,
            #[allow(clippy::const_is_empty)]
            completed: !config::TODOIST_API_KEY.is_empty(),
        },
        Task {
            title: "Connect to WiFi network".to_string(),
            description: "Make sure that your WiFi name and password are correct.".to_string(),
            priority: 2,
            order: 3,
            when: TaskSchedule::None,
            color: OctColor::Red,
            duration: None,
            completed: wifi_connected,
        },
        Task {
            title: "Synchronize system time".to_string(),
            description: format!("Wait for NTP to sync your system time correctly, it is currently {}.", chrono::Local::now()),
            priority: 2,
            order: 4,
            when: TaskSchedule::None,
            color: OctColor::Red,
            duration: None,
            completed: chrono::Local::now().timestamp() > 100000,
        },
        Task {
            title: "Synchronize Todoist tasks".to_string(),
            description: "Make sure that your Todoist API key is correctly configured.".to_string(),
            priority: 3,
            order: 5,
            when: TaskSchedule::None,
            color: OctColor::Red,
            duration: None,
            completed: false,
        },
    ]
}