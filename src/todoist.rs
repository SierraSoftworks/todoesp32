use chrono::{Local, Utc};
use embedded_svc::*;
use epd_waveshare::color::OctColor;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use http::Headers;

use crate::markdown;



pub struct TodoistClient {
    api_key: &'static str,
    filter: &'static str,
}

impl TodoistClient {
    pub fn new(api_key: &'static str, filter: &'static str) -> anyhow::Result<Self> {
        Ok(Self { api_key, filter })
    }

    pub fn get_tasks(&self) -> anyhow::Result<Vec<crate::controls::task_list::Task>> {
        ::log::info!("Making GET request to Todoist API (/v2/tasks)");

        let config = &HttpConfiguration {
            crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
            ..Default::default()
        };
        
        let mut client = http::client::Client::wrap(EspHttpConnection::new(config)?);

        let url = format!("https://api.todoist.com/rest/v2/tasks?filter={}", self.filter).replace(" ", "%20");

        let auth_header = format!("Bearer {}", self.api_key);
        let headers = [
            ("authorization", auth_header.as_str()),
        ];
        let mut response = client.request(http::Method::Get, &url, &headers)?.submit()?;

        match response.status() {
            200 => {
                let mut buffer = vec![0; response.content_len().unwrap_or(16 * 1024) as usize];
                let body_size = response.read(&mut buffer)?;
                ::log::info!("Got HTTP {} from Todoist API", response.status());
                ::log::debug!("{}", std::str::from_utf8(&buffer[..body_size]).unwrap());

                let tasks: Vec<Task> = serde_json::from_slice(&buffer[..body_size])?;

                response.release();

                Ok(tasks.into_iter().map(|t| t.into()).collect())
            }
            status => {
                ::log::error!("Unexpected status code from Todoist API: HTTP {}", status);
                Err(anyhow::anyhow!("HTTP {}", status))
            }
        }
    }
}

#[derive(serde::Deserialize, Debug)]
struct Task {
    priority: u8,
    order: u32,
    content: String,
    description: String,
    due: Option<TaskDue>,
    is_completed: bool,
    duration: Option<TaskDuration>,
}

#[derive(serde::Deserialize, Debug)]
struct TaskDue {
    date: String,
    datetime: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct TaskDuration {
    amount: u32,
    unit: String,
}

#[allow(clippy::from_over_into)]
impl Into<crate::controls::task_list::Task> for Task {
    fn into(self) -> crate::controls::task_list::Task {
        crate::controls::task_list::Task {
            title: markdown::strip(&self.content, 100).to_string(),
            description: markdown::strip(self.description.trim().lines().next().unwrap_or_default(), 200).to_string(),
            priority: 4 - self.priority,
            order: self.order,
            color: match self.priority {
                1 => OctColor::White,
                2 => OctColor::Blue,
                3 => OctColor::Orange,
                4 => OctColor::Red,
                _ => OctColor::Black,
            },
            when: self.due.map(|d| d.into()).unwrap_or_default(),
            completed: self.is_completed,
            duration: self.duration.map(|d| d.into()),
        }
    }
}

#[allow(clippy::from_over_into)]
impl Into<crate::controls::task_list::TaskSchedule> for TaskDue {
    fn into(self) -> crate::controls::task_list::TaskSchedule {
        if let Some(datetime) = self.datetime {
            match chrono::NaiveDateTime::parse_and_remainder(&datetime, "%Y-%m-%dT%H:%M:%S") {
                Ok((dt, _)) => return crate::controls::task_list::TaskSchedule::Time(dt.and_local_timezone(Utc).single().map(|d| d.with_timezone(&Local).naive_local()).unwrap_or(dt)),
                Err(e) => {
                    ::log::warn!("Failed to parse datetime '{}' from Todoist API: {}", &datetime, e);
                },
            }
        }

        if let Ok(d) = chrono::NaiveDate::parse_from_str(&self.date, "%Y-%m-%d") {
            crate::controls::task_list::TaskSchedule::Date(d)
        } else {
            crate::controls::task_list::TaskSchedule::None
        }
    }
}

#[allow(clippy::from_over_into)]
impl Into<chrono::Duration> for TaskDuration {
    fn into(self) -> chrono::Duration {
        match self.unit.as_str() {
            "minute" => chrono::Duration::minutes(self.amount as i64),
            "day" => chrono::Duration::days(self.amount as i64),
            _ => chrono::Duration::zero(),
        }
    }
}