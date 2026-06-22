//! Deterministic content fingerprints used to decide when the (slow, power
//! hungry) e-paper panel actually needs refreshing.
//!
//! The standard-library hasher is randomised and unavailable under `no_std`, so
//! we use a tiny [FNV-1a](https://en.wikipedia.org/wiki/Fowler%E2%80%93Noll%E2%80%93Vo_hash_function)
//! hasher. The fingerprints are only ever compared for equality (never stored
//! long-term across versions), so any stable, well-distributed hash will do.

use core::hash::Hasher;

use chrono::{Datelike, NaiveDate};

use crate::snapshot::TaskSnapshot;

/// A small deterministic FNV-1a hasher.
struct Fnv1a(u64);

impl Default for Fnv1a {
    fn default() -> Self {
        // FNV-1a 64-bit offset basis.
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for Fnv1a {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            // FNV-1a 64-bit prime.
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

/// Feed one task's rendered fields into `hasher`.
///
/// Colours are hashed via their stable 4-bit palette index because
/// [`OctColor`](epd_waveshare::color::OctColor) is not `Hash`.
fn hash_snapshot(hasher: &mut impl Hasher, task: &TaskSnapshot) {
    hasher.write_u8(task.marker_color.get_nibble());
    hasher.write(task.title.as_bytes());
    hasher.write_u8(0);
    match &task.description {
        Some(text) => {
            hasher.write_u8(1);
            hasher.write(text.as_bytes());
            hasher.write_u8(0);
        }
        None => hasher.write_u8(2),
    }
    hasher.write(task.when.as_bytes());
    hasher.write_u8(task.when_color.get_nibble());
    match &task.duration {
        Some(text) => {
            hasher.write_u8(1);
            hasher.write(text.as_bytes());
            hasher.write_u8(0);
        }
        None => hasher.write_u8(2),
    }
}

/// Fingerprint the task screen: the date plus every task's rendered fields.
///
/// Two task screens with the same fingerprint look identical, so the firmware
/// can leave the panel untouched when this value is unchanged.
pub fn fingerprint_tasks(date: NaiveDate, tasks: &[TaskSnapshot]) -> u64 {
    let mut hasher = Fnv1a::default();
    hasher.write_u8(b'T');
    hasher.write_i32(date.num_days_from_ce());
    hasher.write_usize(tasks.len());
    for task in tasks {
        hash_snapshot(&mut hasher, task);
    }
    hasher.finish()
}

/// Fingerprint a status screen identified by `code`.
///
/// Always distinct from any [`fingerprint_tasks`] value, so switching between a
/// status screen and the task list (or between two different status screens)
/// always redraws.
pub fn fingerprint_status(code: u8) -> u64 {
    let mut hasher = Fnv1a::default();
    hasher.write_u8(b'S');
    hasher.write_u8(code);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::Colour;
    use alloc::string::ToString;

    fn snapshot(title: &str) -> TaskSnapshot {
        TaskSnapshot {
            marker_color: Colour::Black,
            title: title.to_string(),
            description: None,
            when: "today".to_string(),
            when_color: Colour::Black,
            duration: None,
        }
    }

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn identical_content_has_the_same_fingerprint() {
        let day = date(2021, 1, 1);
        assert_eq!(
            fingerprint_tasks(day, &[snapshot("a"), snapshot("b")]),
            fingerprint_tasks(day, &[snapshot("a"), snapshot("b")]),
        );
    }

    #[test]
    fn changing_a_task_changes_the_fingerprint() {
        let day = date(2021, 1, 1);
        assert_ne!(
            fingerprint_tasks(day, &[snapshot("a")]),
            fingerprint_tasks(day, &[snapshot("b")]),
        );
    }

    #[test]
    fn reordering_tasks_changes_the_fingerprint() {
        let day = date(2021, 1, 1);
        assert_ne!(
            fingerprint_tasks(day, &[snapshot("a"), snapshot("b")]),
            fingerprint_tasks(day, &[snapshot("b"), snapshot("a")]),
        );
    }

    #[test]
    fn changing_the_date_changes_the_fingerprint() {
        assert_ne!(
            fingerprint_tasks(date(2021, 1, 1), &[snapshot("a")]),
            fingerprint_tasks(date(2021, 1, 2), &[snapshot("a")]),
        );
    }

    #[test]
    fn status_fingerprints_are_distinct() {
        assert_ne!(fingerprint_status(0), fingerprint_status(1));
        assert_ne!(
            fingerprint_status(0),
            fingerprint_tasks(date(2021, 1, 1), &[]),
        );
    }
}
