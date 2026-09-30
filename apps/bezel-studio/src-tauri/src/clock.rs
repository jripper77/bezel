//! The wall clock and language the renderer's clock elements use.

use bezel_core::domain::clock::{Language, LocalTime};
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

/// The language of day and month names for a locale such as `pt-BR`.
pub fn language_of(locale: Option<&str>) -> Language {
    match locale {
        Some(l) if l.to_ascii_lowercase().starts_with("pt") => Language::PortugueseBr,
        _ => Language::English,
    }
}

/// The user's language, from the system locale.
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
    fn portuguese_locales_pick_portuguese() {
        assert_eq!(language_of(Some("pt-BR")), Language::PortugueseBr);
        assert_eq!(language_of(Some("PT_pt")), Language::PortugueseBr);
        assert_eq!(language_of(Some("en-US")), Language::English);
        assert_eq!(language_of(None), Language::English);
        let _ = language();
    }
}
