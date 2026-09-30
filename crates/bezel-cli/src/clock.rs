//! The wall clock and language clock elements use (the core never reads the
//! clock: time is a value passed in).

use bezel_core::domain::clock::{Language, LocalTime, language_of};
use chrono::{Datelike, Local, Timelike};

/// The local time now.
pub fn now() -> LocalTime {
    let t = Local::now();
    // chrono keeps each field in its range, so the narrowing never truncates.
    let narrow = |v: u32| u8::try_from(v).unwrap_or(u8::MAX);
    LocalTime {
        year: t.year(),
        month: narrow(t.month()),
        day: narrow(t.day()),
        hour: narrow(t.hour()),
        minute: narrow(t.minute()),
        second: narrow(t.second()),
        weekday: narrow(t.weekday().num_days_from_monday()),
    }
}

/// The user's language, from the system locale (the core's `language_of`).
pub fn language() -> Language {
    language_of(sys_locale::get_locale().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_in_range() {
        let t = now();
        assert!((1..=12).contains(&t.month) && (1..=31).contains(&t.day));
        assert!(t.hour < 24 && t.minute < 60 && t.second < 61 && t.weekday < 7);
    }

    #[test]
    fn the_system_locale_gives_a_language() {
        let _ = language();
    }
}
