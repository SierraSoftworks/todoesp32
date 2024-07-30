use std::{cmp::Ordering, fmt::Display};

use chrono::NaiveDate;
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

    pub fn get_tasks(&self) -> anyhow::Result<Vec<Task>> {
        ::log::info!("Making GET request to Todoist API (/v2/tasks)");

        let config = &HttpConfiguration {
            crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
            ..Default::default()
        };

        let mut client = http::client::Client::wrap(EspHttpConnection::new(config)?);

        let url = format!(
            "https://api.todoist.com/rest/v2/tasks?filter={}",
            self.filter
        )
        .replace(" ", "%20");

        let auth_header = format!("Bearer {}", self.api_key);
        let headers = [("authorization", auth_header.as_str())];
        let mut response = client
            .request(http::Method::Get, &url, &headers)?
            .submit()?;

        match response.status() {
            200 => {
                let mut buffer = vec![0; response.content_len().unwrap_or(16 * 1024) as usize];
                let body_size = response.read(&mut buffer)?;
                ::log::info!("Got HTTP {} from Todoist API", response.status());
                ::log::debug!("{}", std::str::from_utf8(&buffer[..body_size]).unwrap());

                let mut tasks: Vec<Task> = serde_json::from_slice(&buffer[..body_size])?;
                tasks.sort();

                Ok(tasks)
            }
            status => {
                ::log::error!("Unexpected status code from Todoist API: HTTP {}", status);
                Err(anyhow::anyhow!("HTTP {}", status))
            }
        }
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct Task {
    id: String,
    priority: u8,
    order: i32,
    content: String,
    description: String,
    due: Option<TaskDue>,
    is_completed: bool,
    duration: Option<TaskDuration>,
}

impl Eq for Task {}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.due.as_ref(), other.due.as_ref()) {
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
        .then_with(|| self.priority.cmp(&other.priority).reverse())
        .then_with(|| self.order.cmp(&other.order))
    }
}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[allow(clippy::from_over_into)]
impl Into<crate::controls::TaskSnapshot> for Task {
    fn into(self) -> crate::controls::TaskSnapshot {
        let duration: Option<chrono::Duration> = self.duration.as_ref().map(|d| d.into());

        crate::controls::TaskSnapshot {
            title: markdown::strip(&self.content, 80).to_string(),
            description: if self.description.is_empty() {
                None
            } else {
                Some(
                    markdown::strip(
                        self.description.trim().lines().next().unwrap_or_default(),
                        120,
                    )
                    .to_string(),
                )
            },
            when: match self.due.as_ref() {
                Some(due) => due.to_string(),
                None => "todo".to_string(),
            },
            when_color: if self
                .due
                .as_ref()
                .map(|d| d.is_past(duration))
                .unwrap_or_default()
            {
                OctColor::Red
            } else {
                OctColor::Black
            },
            duration: self.duration.map(|d| d.to_string()),
            marker_color: if self.is_completed {
                OctColor::Green
            } else {
                match self.priority {
                    1 => OctColor::White,
                    2 => OctColor::Blue,
                    3 => OctColor::Orange,
                    4 => OctColor::Red,
                    _ => OctColor::Black,
                }
            },
        }
    }
}

#[derive(serde::Deserialize, Debug, PartialEq, Eq)]
pub struct TaskDue {
    date: String,
    datetime: Option<String>,
    timezone: Option<String>,
}

impl TaskDue {
    pub fn is_past(&self, duration: Option<chrono::Duration>) -> bool {
        let now = chrono::Local::now();

        if let Some(datetime) = self.datetime.as_deref().and_then(|dt| {
            if dt.ends_with('Z') {
                chrono::DateTime::<chrono::FixedOffset>::parse_from_rfc3339(dt)
                    .map(|dt| dt.with_timezone(&chrono::Local).naive_local())
                    .ok()
            } else {
                chrono::NaiveDateTime::parse_from_str(dt, "%Y-%m-%dT%H:%M:%S")
                    .ok()
            }
        }) {
            datetime + duration.unwrap_or_default() < now.naive_utc()
        } else if let Ok(date) = chrono::NaiveDate::parse_from_str(&self.date, "%Y-%m-%d") {
            date < now.date_naive()
        } else {
            false
        }
    }
}

impl Ord for TaskDue {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.datetime.as_deref(), other.datetime.as_deref()) {
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            _ => Ordering::Equal,
        }
        .then_with(|| self.date.cmp(&other.date))
    }
}

impl PartialOrd for TaskDue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[allow(clippy::from_over_into)]
impl TryInto<chrono::NaiveDate> for &TaskDue {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<chrono::NaiveDate, Self::Error> {
        NaiveDate::parse_from_str(&self.date, "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("Invalid date format '{}'", self.date))
    }
}

impl Display for TaskDue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let now = chrono::Local::now();

        if let Some(datetime) = self
            .datetime
            .as_deref()
            .and_then(|dt| {
                chrono::NaiveDateTime::parse_and_remainder(dt, "%Y-%m-%dT%H:%M:%S")
                    .map(|(v, _)| v)
                    .ok()
            })
            .map(|dt| {
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc)
                    .with_timezone(&chrono::Local)
            })
        {
            match datetime.naive_local().date().cmp(&now.naive_local().date()) {
                Ordering::Less => write!(f, "{}", datetime.format("%d/%m")),
                Ordering::Equal => write!(f, "{}", datetime.format("%H:%M")),
                Ordering::Greater => write!(f, "todo"),
            }
        } else if let Ok(date) = chrono::NaiveDate::parse_from_str(&self.date, "%Y-%m-%d") {
            match date.cmp(&now.naive_local().date()) {
                Ordering::Less => write!(f, "{}", date.format("%d/%m")),
                Ordering::Equal => write!(f, "today"),
                Ordering::Greater => write!(f, "todo"),
            }
        } else {
            write!(f, "todo")
        }
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct TaskDuration {
    amount: u32,
    unit: String,
}

#[allow(clippy::from_over_into)]
impl Into<chrono::TimeDelta> for &TaskDuration {
    fn into(self) -> chrono::TimeDelta {
        match self.unit.as_str() {
            "minute" => chrono::TimeDelta::minutes(self.amount as i64),
            "day" => chrono::TimeDelta::days(self.amount as i64),
            _ => chrono::TimeDelta::zero(),
        }
    }
}

impl Display for TaskDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}",
            self.amount,
            match self.unit.as_str() {
                "minute" => "m",
                "day" => "d",
                unit => unit,
            }
        )
    }
}
