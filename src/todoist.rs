use std::{cmp::Ordering, fmt::Display};

use chrono::NaiveDate;
use embedded_svc::*;
use epd_waveshare::color::OctColor;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use io::{Error, ErrorType};

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
        let response = client
            .request(http::Method::Get, &url, &headers)?
            .submit()?;

        match response.status() {
            200 => {
                ::log::info!("Got HTTP {} from Todoist API", response.status());
                let mut tasks: Vec<Task> = serde_json::from_reader(ResponseReader { response })?;
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

struct ResponseReader<C> {
    response: http::client::Response<C>,
}

impl<C: ErrorType> ErrorType for ResponseReader<C> {
    type Error = C::Error;
}

impl<C: http::client::Connection + ErrorType> std::io::Read for ResponseReader<C> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.response.read(buf).map_err(|e| {
            std::io::Error::new(
                match e.kind() {
                    io::ErrorKind::AddrInUse => std::io::ErrorKind::AddrInUse,
                    io::ErrorKind::AddrNotAvailable => std::io::ErrorKind::AddrNotAvailable,
                    io::ErrorKind::AlreadyExists => std::io::ErrorKind::AlreadyExists,
                    io::ErrorKind::BrokenPipe => std::io::ErrorKind::BrokenPipe,
                    io::ErrorKind::ConnectionAborted => std::io::ErrorKind::ConnectionAborted,
                    io::ErrorKind::ConnectionRefused => std::io::ErrorKind::ConnectionRefused,
                    io::ErrorKind::ConnectionReset => std::io::ErrorKind::ConnectionReset,
                    io::ErrorKind::Interrupted => std::io::ErrorKind::Interrupted,
                    io::ErrorKind::InvalidData => std::io::ErrorKind::InvalidData,
                    io::ErrorKind::InvalidInput => std::io::ErrorKind::InvalidInput,
                    io::ErrorKind::NotConnected => std::io::ErrorKind::NotConnected,
                    io::ErrorKind::NotFound => std::io::ErrorKind::NotFound,
                    io::ErrorKind::OutOfMemory => std::io::ErrorKind::OutOfMemory,
                    io::ErrorKind::PermissionDenied => std::io::ErrorKind::PermissionDenied,
                    io::ErrorKind::TimedOut => std::io::ErrorKind::TimedOut,
                    io::ErrorKind::Unsupported => std::io::ErrorKind::Unsupported,
                    io::ErrorKind::WriteZero => std::io::ErrorKind::WriteZero,
                    _ => std::io::ErrorKind::Other,
                },
                format!("{:?}", e),
            )
        })
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

        let state = self
            .due
            .map(|due| due.state(duration))
            .unwrap_or(TaskDueState::Unknown);

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
            when: format!("{}", &state),
            when_color: match state {
                TaskDueState::NowTime => OctColor::Green,
                TaskDueState::PastDate(..) | TaskDueState::PastTime(..) => OctColor::Red,
                _ => OctColor::Black,
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
    pub fn state(&self, duration: Option<chrono::Duration>) -> TaskDueState {
        let now = chrono::Local::now();

        match self.try_into() {
            Ok(dt @ chrono::DateTime::<chrono::Local> { .. })
                if dt + duration.unwrap_or_default() < now =>
            {
                TaskDueState::PastTime(dt.naive_local())
            }
            Ok(dt @ chrono::DateTime::<chrono::Local> { .. }) if dt < now => TaskDueState::NowTime,
            Ok(dt @ chrono::DateTime::<chrono::Local> { .. }) => {
                TaskDueState::FutureTime(dt.naive_local())
            }
            Err(_) => match self.try_into() {
                Ok(date @ chrono::NaiveDate { .. }) if date < now.date_naive() => {
                    TaskDueState::PastDate(date)
                }
                Ok(date @ chrono::NaiveDate { .. }) if date == now.date_naive() => {
                    TaskDueState::NowDate
                }
                Ok(date @ chrono::NaiveDate { .. }) => TaskDueState::FutureDate(date),
                Err(_) => TaskDueState::Unknown,
            },
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

impl TryInto<chrono::DateTime<chrono::Local>> for &TaskDue {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<chrono::DateTime<chrono::Local>, Self::Error> {
        if let Some(dt) = self.datetime.as_deref() {
            if dt.ends_with('Z') {
                chrono::DateTime::<chrono::FixedOffset>::parse_from_rfc3339(dt)
                    .map(|dt| dt.with_timezone(&chrono::Local))
                    .map_err(|e| anyhow::anyhow!("Invalid datetime format '{dt}': {e}"))
            } else {
                chrono::NaiveDateTime::parse_from_str(dt, "%Y-%m-%dT%H:%M:%S")
                    .map_err(|e| anyhow::anyhow!("Invalid datetime format '{}': {:?}", dt, e))
                    .and_then(|dt| {
                        dt.and_local_timezone(chrono::Local)
                            .single()
                            .ok_or_else(|| anyhow::anyhow!("Cannot set timezone to local"))
                    })
                    .map_err(|e| anyhow::anyhow!("Invalid datetime format '{dt}': {e}"))
            }
        } else {
            Err(anyhow::anyhow!("No datetime field"))
        }
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

pub enum TaskDueState {
    Unknown,
    PastDate(chrono::NaiveDate),
    NowDate,
    FutureDate(chrono::NaiveDate),
    PastTime(chrono::NaiveDateTime),
    NowTime,
    FutureTime(chrono::NaiveDateTime),
}

impl Display for TaskDueState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let now = chrono::Local::now();

        match self {
            Self::Unknown => write!(f, "todo"),
            Self::PastDate(date) => write!(f, "{}", date.format("%d/%m")),
            Self::NowDate => write!(f, "today"),
            Self::FutureDate(date) => write!(f, "{}", date.format("%d/%m")),
            Self::PastTime(datetime) if datetime.date() == now.naive_local().date() => {
                write!(f, "{}", datetime.format("%H:%M"))
            }
            Self::PastTime(datetime) => write!(f, "{}", datetime.format("%d/%m")),
            Self::NowTime => write!(f, "now"),
            Self::FutureTime(datetime) if datetime.date() == now.naive_local().date() => {
                write!(f, "{}", datetime.format("%H:%M"))
            }
            Self::FutureTime(datetime) => write!(f, "{}", datetime.format("%d/%m")),
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
