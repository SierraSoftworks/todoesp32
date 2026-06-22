//! Todoist task model, JSON parsing, ordering and due-date handling.
//!
//! All time-dependent behaviour takes an explicit `now: DateTime<FixedOffset>`
//! argument so that it is deterministic and host-testable (there is no
//! `chrono::Local` in the firmware).

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cmp::Ordering;

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone};
use hifijson::num::{Lex as _, LexWrite as _};
use hifijson::str::{Lex as _, LexAlloc as _};
use hifijson::token::Lex;
use hifijson::{Expect, SliceLexer};

use crate::colour::Colour;
use crate::markdown;
use crate::snapshot::TaskSnapshot;

/// Parse a Todoist "filter" API response and return the tasks sorted into
/// display order.
///
/// This convenience wrapper buffers the whole response in `json`. The firmware
/// instead drives a [`TaskStreamParser`] directly so it can parse the response
/// incrementally off the network without ever allocating a buffer for the whole
/// (potentially multi-kilobyte) body.
pub fn parse_tasks(json: &[u8]) -> Result<Vec<Task>, ParseError> {
    let mut parser = TaskStreamParser::new();
    let mut tasks = Vec::new();
    parser.feed(json, &mut tasks)?;
    tasks.sort();
    Ok(tasks)
}

/// An error encountered while parsing a Todoist task object.
///
/// The JSON tokeniser ([`hifijson`]) and this crate's hand-written deserialiser
/// surface every failure (malformed JSON, an unexpected value type, a missing
/// required field, an out-of-range number, ...) as this single opaque error;
/// the firmware only needs to know that parsing failed, not why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseError;

impl From<Expect> for ParseError {
    fn from(_: Expect) -> Self {
        ParseError
    }
}

/// The object key whose array value holds the individual task objects.
const RESULTS_KEY: &[u8] = b"\"results\"";

/// Incrementally extracts task objects from a Todoist "filter" API response.
///
/// The response has the shape `{"results":[{..task..},{..task..}],...}`. Rather
/// than buffer the entire (potentially large) body, callers feed the response
/// bytes in arbitrarily sized chunks via [`feed`](Self::feed). Each complete
/// object found inside the `results` array is parsed with
/// [`hifijson`] (via this crate's hand-written deserialiser) and pushed to the
/// output vector, so only a single task object is ever buffered at a time —
/// which keeps peak memory small enough for the device's tiny heap.
///
/// The caller is responsible for sorting the collected tasks (see
/// [`Task`]'s [`Ord`] implementation) once the stream is exhausted.
#[derive(Default)]
pub struct TaskStreamParser {
    phase: Phase,
    /// How many leading bytes of [`RESULTS_KEY`] have matched so far.
    key_idx: usize,
    /// Bytes of the object currently being accumulated (reused between objects).
    object: Vec<u8>,
    /// Brace-nesting depth within the current object.
    depth: u32,
    /// Whether the cursor is inside a JSON string literal.
    in_string: bool,
    /// Whether the previous byte was an unescaped backslash inside a string.
    escaped: bool,
}

#[derive(Default)]
enum Phase {
    /// Scanning the leading bytes for the `"results"` key.
    #[default]
    SeekKey,
    /// Found the key; skipping `:`/whitespace until the opening `[`.
    SeekArray,
    /// Inside the array, skipping whitespace/commas between elements.
    BetweenElements,
    /// Accumulating the bytes of one task object.
    InObject,
    /// Reached the array's closing `]`; any further bytes are ignored.
    Done,
}

impl TaskStreamParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk of response bytes, appending any newly completed tasks to
    /// `out`. May be called repeatedly as data arrives off the network.
    pub fn feed(&mut self, chunk: &[u8], out: &mut Vec<Task>) -> Result<(), ParseError> {
        for &byte in chunk {
            self.feed_byte(byte, out)?;
        }
        Ok(())
    }

    fn feed_byte(&mut self, byte: u8, out: &mut Vec<Task>) -> Result<(), ParseError> {
        match self.phase {
            Phase::SeekKey => {
                if byte == RESULTS_KEY[self.key_idx] {
                    self.key_idx += 1;
                    if self.key_idx == RESULTS_KEY.len() {
                        self.phase = Phase::SeekArray;
                    }
                } else {
                    // Restart the match, allowing the current byte to begin a
                    // fresh one. (`"results"` has no self-overlap, and only `{`
                    // precedes it in a real response, so this is sufficient.)
                    self.key_idx = usize::from(byte == RESULTS_KEY[0]);
                }
            }
            Phase::SeekArray => {
                if byte == b'[' {
                    self.phase = Phase::BetweenElements;
                }
            }
            Phase::BetweenElements => match byte {
                b'{' => {
                    self.object.clear();
                    self.object.push(b'{');
                    self.depth = 1;
                    self.in_string = false;
                    self.escaped = false;
                    self.phase = Phase::InObject;
                }
                b']' => self.phase = Phase::Done,
                _ => {} // whitespace or ',' between elements
            },
            Phase::InObject => {
                self.object.push(byte);
                if self.in_string {
                    if self.escaped {
                        self.escaped = false;
                    } else {
                        match byte {
                            b'\\' => self.escaped = true,
                            b'"' => self.in_string = false,
                            _ => {}
                        }
                    }
                } else {
                    match byte {
                        b'"' => self.in_string = true,
                        b'{' => self.depth += 1,
                        b'}' => {
                            self.depth = self.depth.saturating_sub(1);
                            if self.depth == 0 {
                                let task = parse_task_object(&self.object)?;
                                out.push(task);
                                self.phase = Phase::BetweenElements;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Phase::Done => {}
        }
        Ok(())
    }
}

// --- A tiny JSON deserialisation framework -----------------------------------
//
// hifijson does the tokenising; [`FromJson`] turns those tokens directly into
// our types without an intermediate DOM and without serde. Implementations are
// provided below for the primitive JSON types and for `Option<T>`, and the
// Todoist model types compose them. Only a single task object is ever
// materialised at a time, keeping peak memory small enough for the tiny heap.

/// Parse exactly one Todoist task object from its raw JSON bytes.
fn parse_task_object(bytes: &[u8]) -> Result<Task, ParseError> {
    let mut lexer = SliceLexer::new(bytes);
    lexer.exactly_one(Lex::ws_peek, Task::from_json)
}

/// A type that can be parsed from a single JSON value.
///
/// `next` is the already-peeked first non-whitespace byte of the value (the
/// `seq`/`exactly_one` combinators hand it over); the implementation must
/// consume exactly that one value from `lexer`.
pub trait FromJson: Sized {
    fn from_json(next: u8, lexer: &mut SliceLexer<'_>) -> Result<Self, ParseError>;
}

impl FromJson for String {
    fn from_json(next: u8, lexer: &mut SliceLexer<'_>) -> Result<Self, ParseError> {
        if next != b'"' {
            return Err(ParseError);
        }
        // `str_string` interprets escape sequences (borrowing from the input
        // when it can), so a string allocates at most once — here.
        let value = lexer.discarded().str_string().map_err(|_| ParseError)?;
        Ok(String::from(&*value))
    }
}

impl FromJson for bool {
    fn from_json(_next: u8, lexer: &mut SliceLexer<'_>) -> Result<Self, ParseError> {
        match lexer.null_or_bool() {
            Some(Some(value)) => Ok(value),
            _ => Err(ParseError),
        }
    }
}

/// `null` parses to `None`; any other value is delegated to `T`.
impl<T: FromJson> FromJson for Option<T> {
    fn from_json(next: u8, lexer: &mut SliceLexer<'_>) -> Result<Self, ParseError> {
        if next == b'n' {
            return match lexer.null_or_bool() {
                Some(None) => Ok(None),
                _ => Err(ParseError),
            };
        }
        T::from_json(next, lexer).map(Some)
    }
}

/// Implement [`FromJson`] for integer types by lexing the number and parsing
/// its textual form.
macro_rules! impl_from_json_int {
    ($($t:ty),* $(,)?) => {
        $(impl FromJson for $t {
            fn from_json(_next: u8, lexer: &mut SliceLexer<'_>) -> Result<Self, ParseError> {
                let (number, _parts) = lexer.num_string().validated().map_err(|_| ParseError)?;
                let text: &str = number.as_ref();
                text.parse::<$t>().map_err(|_| ParseError)
            }
        })*
    };
}

impl_from_json_int!(u8, u16, u32, u64, i8, i16, i32, i64);

/// Read a JSON object, invoking `on_field` for each member. When it is called,
/// the lexer is positioned on the (already-peeked) field value passed as
/// `value_next`, and `on_field` must consume exactly that value. The peeked
/// `next` must be the opening `{`.
fn read_object<F>(next: u8, lexer: &mut SliceLexer<'_>, mut on_field: F) -> Result<(), ParseError>
where
    F: FnMut(&str, u8, &mut SliceLexer<'_>) -> Result<(), ParseError>,
{
    if next != b'{' {
        return Err(ParseError);
    }
    lexer
        .discarded()
        .seq(b'}', Lex::ws_peek, |key_next, lexer| {
            if key_next != b'"' {
                return Err(ParseError);
            }
            let key = lexer.discarded().str_string().map_err(|_| ParseError)?;
            let key: &str = &key;
            lexer.expect(Lex::ws_peek, b':').ok_or(ParseError)?;
            let value_next = lexer.ws_peek().ok_or(ParseError)?;
            on_field(key, value_next, lexer)
        })
}

/// Recursively discard a single JSON value of any type.
fn skip_value(next: u8, lexer: &mut SliceLexer<'_>) -> Result<(), ParseError> {
    match next {
        b'"' => {
            lexer.discarded().str_ignore().map_err(|_| ParseError)?;
        }
        b'-' | b'0'..=b'9' => {
            lexer.num_ignore().validate().map_err(|_| ParseError)?;
        }
        b't' | b'f' | b'n' => {
            lexer.null_or_bool().ok_or(ParseError)?;
        }
        b'[' => {
            lexer.discarded().seq(b']', Lex::ws_peek, skip_value)?;
        }
        b'{' => {
            read_object(next, lexer, |_key, value_next, lexer| {
                skip_value(value_next, lexer)
            })?;
        }
        _ => return Err(ParseError),
    }
    Ok(())
}

impl FromJson for Task {
    fn from_json(next: u8, lexer: &mut SliceLexer<'_>) -> Result<Self, ParseError> {
        let mut builder = TaskBuilder::default();
        read_object(next, lexer, |key, value_next, lexer| {
            match key {
                "id" => builder.id = Some(FromJson::from_json(value_next, lexer)?),
                "priority" => builder.priority = Some(FromJson::from_json(value_next, lexer)?),
                "child_order" => {
                    builder.child_order = Some(FromJson::from_json(value_next, lexer)?)
                }
                "content" => builder.content = Some(FromJson::from_json(value_next, lexer)?),
                "description" => {
                    builder.description = Some(FromJson::from_json(value_next, lexer)?)
                }
                "due" => builder.due = FromJson::from_json(value_next, lexer)?,
                "checked" => builder.checked = Some(FromJson::from_json(value_next, lexer)?),
                "duration" => builder.duration = FromJson::from_json(value_next, lexer)?,
                _ => skip_value(value_next, lexer)?,
            }
            Ok(())
        })?;
        builder.build()
    }
}

/// Collects task fields as they are parsed, then validates the required ones.
#[derive(Default)]
struct TaskBuilder {
    id: Option<String>,
    priority: Option<u8>,
    child_order: Option<i32>,
    content: Option<String>,
    description: Option<String>,
    due: Option<TaskDue>,
    checked: Option<bool>,
    duration: Option<TaskDuration>,
}

impl TaskBuilder {
    /// Build the task, requiring only `id` and `content`; every other field
    /// falls back to a sensible default so a single missing field never
    /// discards the whole response.
    fn build(self) -> Result<Task, ParseError> {
        Ok(Task {
            id: self.id.ok_or(ParseError)?,
            priority: self.priority.unwrap_or(1),
            child_order: self.child_order.unwrap_or(0),
            content: self.content.ok_or(ParseError)?,
            description: self.description.unwrap_or_default(),
            due: self.due,
            checked: self.checked.unwrap_or(false),
            duration: self.duration,
        })
    }
}

impl FromJson for TaskDue {
    fn from_json(next: u8, lexer: &mut SliceLexer<'_>) -> Result<Self, ParseError> {
        let mut date: Option<String> = None;
        let mut timezone: Option<String> = None;
        read_object(next, lexer, |key, value_next, lexer| {
            match key {
                "date" => date = Some(FromJson::from_json(value_next, lexer)?),
                "timezone" => timezone = FromJson::from_json(value_next, lexer)?,
                _ => skip_value(value_next, lexer)?,
            }
            Ok(())
        })?;
        Ok(TaskDue {
            date: date.ok_or(ParseError)?,
            timezone,
        })
    }
}

impl FromJson for TaskDuration {
    fn from_json(next: u8, lexer: &mut SliceLexer<'_>) -> Result<Self, ParseError> {
        let mut amount: Option<u32> = None;
        let mut unit: Option<String> = None;
        read_object(next, lexer, |key, value_next, lexer| {
            match key {
                "amount" => amount = Some(FromJson::from_json(value_next, lexer)?),
                "unit" => unit = Some(FromJson::from_json(value_next, lexer)?),
                _ => skip_value(value_next, lexer)?,
            }
            Ok(())
        })?;
        Ok(TaskDuration {
            amount: amount.ok_or(ParseError)?,
            unit: unit.ok_or(ParseError)?,
        })
    }
}

#[derive(Debug)]
pub struct Task {
    id: String,
    priority: u8,
    child_order: i32,
    content: String,
    description: String,
    due: Option<TaskDue>,
    checked: bool,
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
        .then_with(|| self.child_order.cmp(&other.child_order))
    }
}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Task {
    /// Convert the task into a renderer-agnostic [`TaskSnapshot`], computing the
    /// "when" label and colours relative to `now`.
    pub fn into_snapshot(self, now: DateTime<FixedOffset>) -> TaskSnapshot {
        let duration: Option<TimeDelta> = self.duration.as_ref().map(|d| d.into());

        let state = self
            .due
            .map(|due| due.state(now, duration))
            .unwrap_or(TaskDueState::Unknown);

        TaskSnapshot {
            title: markdown::strip(self.content.as_str(), 80),
            description: if self.description.is_empty() {
                None
            } else {
                Some(markdown::strip(
                    self.description.trim().lines().next().unwrap_or_default(),
                    120,
                ))
            },
            when: state.format(now),
            when_color: match state {
                TaskDueState::NowTime => Colour::Green,
                TaskDueState::PastDate(..) | TaskDueState::PastTime(..) => Colour::Red,
                _ => Colour::Black,
            },
            duration: self.duration.map(|d| d.label()),
            marker_color: if self.checked {
                Colour::Green
            } else {
                match self.priority {
                    1 => Colour::White,
                    2 => Colour::Blue,
                    3 => Colour::Orange,
                    4 => Colour::Red,
                    _ => Colour::Black,
                }
            },
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskDue {
    date: String,
    timezone: Option<String>,
}

impl TaskDue {
    /// Compute the display state of this due date relative to `now`.
    pub fn state(&self, now: DateTime<FixedOffset>, duration: Option<TimeDelta>) -> TaskDueState {
        match self.to_datetime(now.offset()) {
            Some(dt) if dt + duration.unwrap_or_default() < now => {
                TaskDueState::PastTime(dt.naive_local())
            }
            Some(dt) if dt < now => TaskDueState::NowTime,
            Some(dt) => TaskDueState::FutureTime(dt.naive_local()),
            None => match self.to_date() {
                Some(date) if date < now.date_naive() => TaskDueState::PastDate(date),
                Some(date) if date == now.date_naive() => TaskDueState::NowDate,
                Some(date) => TaskDueState::FutureDate(date),
                None => TaskDueState::Unknown,
            },
        }
    }

    /// Parse the due date as a concrete instant, if it carries a time component.
    ///
    /// Floating due dates (no timezone) are interpreted in the supplied
    /// `offset`; fixed due dates are stored in UTC (RFC 3339) and converted.
    fn to_datetime(&self, offset: &FixedOffset) -> Option<DateTime<FixedOffset>> {
        // Only due dates that carry a time component (e.g. `2018-11-15T12:00:00`)
        // can be turned into a concrete instant.
        if !self.date.contains('T') {
            return None;
        }

        let dt = self.date.as_str();
        if dt.ends_with('Z') {
            // Due dates with a fixed timezone are stored in UTC (RFC 3339).
            DateTime::parse_from_rfc3339(dt)
                .ok()
                .map(|dt| dt.with_timezone(offset))
        } else {
            // Floating due dates have no timezone and may carry fractional
            // seconds (e.g. `2018-11-15T12:00:00.000000`); interpret them in the
            // configured local timezone.
            let dt_no_frac = dt.split('.').next().unwrap_or(dt);
            NaiveDateTime::parse_from_str(dt_no_frac, "%Y-%m-%dT%H:%M:%S")
                .ok()
                .and_then(|ndt| offset.from_local_datetime(&ndt).single())
        }
    }

    /// Parse the due date as a plain calendar date.
    fn to_date(&self) -> Option<NaiveDate> {
        let date = self.date.split('T').next().unwrap_or(&self.date);
        NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
    }
}

impl Ord for TaskDue {
    fn cmp(&self, other: &Self) -> Ordering {
        // The `date` field holds either a `YYYY-MM-DD` value or a datetime
        // (`YYYY-MM-DDTHH:MM:SS[Z]`). Both share the same leading layout, so a
        // lexical comparison orders them chronologically (all-day items, which
        // lack a time component, sort before timed items on the same day).
        self.date.cmp(&other.date)
    }
}

impl PartialOrd for TaskDue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The temporal state of a task's due date, relative to "now".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskDueState {
    Unknown,
    PastDate(NaiveDate),
    NowDate,
    FutureDate(NaiveDate),
    PastTime(NaiveDateTime),
    NowTime,
    FutureTime(NaiveDateTime),
}

impl TaskDueState {
    /// Render the short "when" label shown next to a task, relative to `now`.
    pub fn format(&self, now: DateTime<FixedOffset>) -> String {
        let today = now.naive_local().date();
        match self {
            Self::Unknown => "todo".to_string(),
            Self::PastDate(date) => date.format("%d/%m").to_string(),
            Self::NowDate => "today".to_string(),
            Self::FutureDate(date) => date.format("%d/%m").to_string(),
            Self::PastTime(datetime) if datetime.date() == today => {
                datetime.format("%H:%M").to_string()
            }
            Self::PastTime(datetime) => datetime.format("%d/%m").to_string(),
            Self::NowTime => "now".to_string(),
            Self::FutureTime(datetime) if datetime.date() == today => {
                datetime.format("%H:%M").to_string()
            }
            Self::FutureTime(datetime) => datetime.format("%d/%m").to_string(),
        }
    }
}

#[derive(Debug)]
pub struct TaskDuration {
    amount: u32,
    unit: String,
}

impl TaskDuration {
    /// A short human label such as `30m` or `2d`.
    pub fn label(&self) -> String {
        alloc::format!(
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

impl From<&TaskDuration> for TimeDelta {
    fn from(value: &TaskDuration) -> Self {
        match value.unit.as_str() {
            "minute" => TimeDelta::minutes(value.amount as i64),
            "day" => TimeDelta::days(value.amount as i64),
            _ => TimeDelta::zero(),
        }
    }
}

/// Helper used internally and by tests to build a `now` value.
#[allow(dead_code)]
fn naive(date: NaiveDate, h: u32, m: u32) -> NaiveDateTime {
    date.and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::offset_from_seconds;

    fn now_at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<FixedOffset> {
        let offset = offset_from_seconds(0);
        offset
            .from_local_datetime(&naive(
                NaiveDate::from_ymd_opt(year, month, day).unwrap(),
                hour,
                minute,
            ))
            .single()
            .unwrap()
    }

    const SAMPLE: &[u8] = br#"{
        "results": [
            {"id":"b","priority":1,"child_order":3,"content":"**Buy** milk","description":"from the *store*","due":{"date":"2021-01-01T09:00:00Z"},"checked":false,"duration":{"amount":30,"unit":"minute"}},
            {"id":"a","priority":4,"child_order":1,"content":"Call mum","description":"","due":{"date":"2021-01-01"},"checked":false,"duration":null},
            {"id":"c","priority":2,"child_order":2,"content":"No due date","description":"","due":null,"checked":false,"duration":null}
        ]
    }"#;

    #[test]
    fn parses_and_sorts_tasks() {
        let tasks = parse_tasks(SAMPLE).expect("valid json");
        assert_eq!(tasks.len(), 3);
        // All-day (a) sorts before timed (b) on the same day, both before the
        // task with no due date (c).
        assert_eq!(tasks[0].id, "a");
        assert_eq!(tasks[1].id, "b");
        assert_eq!(tasks[2].id, "c");
    }

    #[test]
    fn ignores_unknown_fields() {
        let json = br#"{"results":[{"id":"x","priority":1,"child_order":0,"content":"hi","description":"","due":null,"checked":false,"duration":null,"unexpected":42}]}"#;
        let tasks = parse_tasks(json).expect("valid json");
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn streaming_parser_handles_chunk_boundaries() {
        // Feeding the response one byte at a time must yield the same result as
        // a single bulk parse, exercising splits at every position (mid-key,
        // mid-string, mid-object).
        let mut parser = TaskStreamParser::new();
        let mut tasks = Vec::new();
        for &byte in SAMPLE {
            parser.feed(&[byte], &mut tasks).expect("valid json");
        }
        tasks.sort();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].id, "a");
        assert_eq!(tasks[1].id, "b");
        assert_eq!(tasks[2].id, "c");
    }

    #[test]
    fn streaming_parser_ignores_braces_and_quotes_in_strings() {
        // Braces, brackets and escaped quotes inside string values must not be
        // mistaken for object boundaries.
        let json = br#"{"results":[{"id":"x","priority":1,"child_order":0,"content":"a {tricky} [value] with \"quotes\"","description":"","due":null,"checked":false,"duration":null}]}"#;
        let mut parser = TaskStreamParser::new();
        let mut tasks = Vec::new();
        parser.feed(json, &mut tasks).expect("valid json");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "x");
    }

    #[test]
    fn streaming_parser_ignores_trailing_fields() {
        // Anything after the closing `]` of the results array is ignored.
        let json = br#"{"results":[{"id":"x","priority":1,"child_order":0,"content":"hi","description":"","due":null,"checked":false,"duration":null}],"next_cursor":"abc123"}"#;
        let mut parser = TaskStreamParser::new();
        let mut tasks = Vec::new();
        parser.feed(json, &mut tasks).expect("valid json");
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn streaming_parser_handles_empty_results() {
        let mut parser = TaskStreamParser::new();
        let mut tasks = Vec::new();
        parser
            .feed(br#"{"results":[]}"#, &mut tasks)
            .expect("valid json");
        assert_eq!(tasks.len(), 0);
    }

    #[test]
    fn all_day_sorts_before_timed_same_day() {
        let all_day = TaskDue {
            date: "2021-01-01".to_string(),
            timezone: None,
        };
        let timed = TaskDue {
            date: "2021-01-01T09:00:00Z".to_string(),
            timezone: None,
        };
        assert_eq!(all_day.cmp(&timed), Ordering::Less);
    }

    #[test]
    fn due_state_today_date() {
        let due = TaskDue {
            date: "2021-01-01".to_string(),
            timezone: None,
        };
        let now = now_at(2021, 1, 1, 12, 0);
        assert_eq!(due.state(now, None), TaskDueState::NowDate);
        assert_eq!(due.state(now, None).format(now), "today");
    }

    #[test]
    fn due_state_past_and_future_dates() {
        let now = now_at(2021, 6, 15, 12, 0);

        let past = TaskDue {
            date: "2021-06-14".to_string(),
            timezone: None,
        };
        assert!(matches!(past.state(now, None), TaskDueState::PastDate(_)));

        let future = TaskDue {
            date: "2021-06-16".to_string(),
            timezone: None,
        };
        assert!(matches!(
            future.state(now, None),
            TaskDueState::FutureDate(_)
        ));
    }

    #[test]
    fn due_state_times_relative_to_now() {
        let now = now_at(2021, 1, 1, 12, 0);

        let earlier = TaskDue {
            date: "2021-01-01T09:00:00Z".to_string(),
            timezone: None,
        };
        assert!(matches!(
            earlier.state(now, None),
            TaskDueState::PastTime(_)
        ));
        // Same day past time renders as HH:MM.
        assert_eq!(earlier.state(now, None).format(now), "09:00");

        let later = TaskDue {
            date: "2021-01-01T15:00:00Z".to_string(),
            timezone: None,
        };
        assert!(matches!(
            later.state(now, None),
            TaskDueState::FutureTime(_)
        ));
        assert_eq!(later.state(now, None).format(now), "15:00");
    }

    #[test]
    fn due_state_now_within_duration() {
        // A task due at 11:45 with a 30 minute duration is still "now" at 12:00.
        let now = now_at(2021, 1, 1, 12, 0);
        let due = TaskDue {
            date: "2021-01-01T11:45:00Z".to_string(),
            timezone: None,
        };
        assert_eq!(
            due.state(now, Some(TimeDelta::minutes(30))),
            TaskDueState::NowTime
        );
    }

    #[test]
    fn duration_label_and_delta() {
        let minutes = TaskDuration {
            amount: 30,
            unit: "minute".to_string(),
        };
        assert_eq!(minutes.label(), "30m");
        assert_eq!(TimeDelta::from(&minutes), TimeDelta::minutes(30));

        let days = TaskDuration {
            amount: 2,
            unit: "day".to_string(),
        };
        assert_eq!(days.label(), "2d");
        assert_eq!(TimeDelta::from(&days), TimeDelta::days(2));
    }

    #[test]
    fn into_snapshot_strips_markdown_and_sets_colours() {
        let tasks = parse_tasks(SAMPLE).expect("valid json");
        let now = now_at(2021, 1, 1, 12, 0);

        let buy = tasks
            .into_iter()
            .find(|t| t.id == "b")
            .unwrap()
            .into_snapshot(now);

        assert_eq!(buy.title, "Buy milk");
        assert_eq!(buy.description.as_deref(), Some("from the store"));
        assert_eq!(buy.duration.as_deref(), Some("30m"));
        // Priority 1 (lowest) renders a white marker.
        assert_eq!(buy.marker_color, Colour::White);
    }
}
