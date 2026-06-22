//! Timezone helpers.
//!
//! `chrono::Local` relies on an operating system timezone database which is not
//! available in a `no_std` firmware. Instead the application is configured with
//! a fixed UTC offset (in seconds) and all "local" times are expressed as
//! [`chrono::DateTime<chrono::FixedOffset>`].

use chrono::{DateTime, FixedOffset, TimeZone};

/// Build a [`FixedOffset`] from a signed number of seconds east of UTC.
///
/// Falls back to UTC if the value is outside the range chrono accepts
/// (±86_400 seconds).
pub fn offset_from_seconds(seconds: i32) -> FixedOffset {
    FixedOffset::east_opt(seconds)
        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC is valid"))
}

/// Convert a Unix timestamp (seconds since the epoch, UTC) into a local
/// [`DateTime`] using the supplied [`FixedOffset`].
pub fn local_from_unix(unix_seconds: i64, offset: FixedOffset) -> Option<DateTime<FixedOffset>> {
    offset.timestamp_opt(unix_seconds, 0).single()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn positive_offset() {
        let offset = offset_from_seconds(2 * 3600);
        assert_eq!(offset, FixedOffset::east_opt(7200).unwrap());
    }

    #[test]
    fn out_of_range_falls_back_to_utc() {
        let offset = offset_from_seconds(i32::MAX);
        assert_eq!(offset, FixedOffset::east_opt(0).unwrap());
    }

    #[test]
    fn converts_unix_timestamp_with_offset() {
        // 2021-01-01T00:00:00Z == 2021-01-01T02:00:00+02:00
        let offset = offset_from_seconds(2 * 3600);
        let dt = local_from_unix(1_609_459_200, offset).unwrap();
        assert_eq!(dt.year(), 2021);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 1);
        assert_eq!(dt.hour(), 2);
        assert_eq!(dt.minute(), 0);
    }
}
