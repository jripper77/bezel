//! Date and time text for clock elements, from a local wall-clock time the
//! driving adapter provides (the core never reads the clock).

/// A local date and time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    /// Year, e.g. 2026.
    pub year: i32,
    /// 1..=12.
    pub month: u8,
    /// 1..=31.
    pub day: u8,
    /// 0..=23.
    pub hour: u8,
    /// 0..=59.
    pub minute: u8,
    /// 0..=59.
    pub second: u8,
    /// 0 = Monday … 6 = Sunday.
    pub weekday: u8,
}

/// Language of day and month names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// English.
    #[default]
    English,
    /// Brazilian Portuguese.
    PortugueseBr,
}

/// The language of day and month names for a locale such as `pt-BR` (the
/// driving adapter reads the system's): Portuguese for any `pt` locale,
/// English otherwise.
pub fn language_of(locale: Option<&str>) -> Language {
    match locale {
        Some(l) if l.to_ascii_lowercase().starts_with("pt") => Language::PortugueseBr,
        _ => Language::English,
    }
}

const EN_DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const PT_DAYS: [&str; 7] = [
    "segunda-feira",
    "terça-feira",
    "quarta-feira",
    "quinta-feira",
    "sexta-feira",
    "sábado",
    "domingo",
];
const PT_DAYS_SHORT: [&str; 7] = ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"];
const EN_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const PT_MONTHS: [&str; 12] = [
    "janeiro",
    "fevereiro",
    "março",
    "abril",
    "maio",
    "junho",
    "julho",
    "agosto",
    "setembro",
    "outubro",
    "novembro",
    "dezembro",
];

fn day_name(t: &LocalTime, lang: Language, short: bool) -> String {
    let i = usize::from(t.weekday.min(6));
    match (lang, short) {
        (Language::English, false) => EN_DAYS[i].to_string(),
        (Language::English, true) => EN_DAYS[i][..3].to_string(),
        (Language::PortugueseBr, false) => PT_DAYS[i].to_string(),
        (Language::PortugueseBr, true) => PT_DAYS_SHORT[i].to_string(),
    }
}

fn month_name(t: &LocalTime, lang: Language, short: bool) -> String {
    let i = usize::from(t.month.clamp(1, 12) - 1);
    let full = match lang {
        Language::English => EN_MONTHS[i],
        Language::PortugueseBr => PT_MONTHS[i],
    };
    if short {
        full.chars().take(3).collect()
    } else {
        full.to_string()
    }
}

/// Formats `t` with a `strftime`-like pattern:
/// `%H` 24-hour, `%I` 12-hour, `%p` AM/PM, `%M` minute, `%S` second,
/// `%d` day, `%e` day without padding, `%m` month, `%Y` year, `%y` 2-digit
/// year, `%A`/`%a` weekday name full/short, `%B`/`%b` month name full/short,
/// `%%` a percent sign. Unknown directives are copied as they are.
pub fn format_clock(pattern: &str, t: &LocalTime, lang: Language) -> String {
    let mut out = String::with_capacity(pattern.len() + 8);
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let Some(d) = chars.next() else {
            out.push('%');
            break;
        };
        let hour12 = match t.hour % 12 {
            0 => 12,
            h => h,
        };
        let piece = match d {
            'H' => format!("{:02}", t.hour),
            'I' => format!("{hour12:02}"),
            'p' => if t.hour < 12 { "AM" } else { "PM" }.to_string(),
            'M' => format!("{:02}", t.minute),
            'S' => format!("{:02}", t.second),
            'd' => format!("{:02}", t.day),
            'e' => t.day.to_string(),
            'm' => format!("{:02}", t.month),
            'Y' => t.year.to_string(),
            'y' => format!("{:02}", t.year.rem_euclid(100)),
            'A' => day_name(t, lang, false),
            'a' => day_name(t, lang, true),
            'B' => month_name(t, lang, false),
            'b' => month_name(t, lang, true),
            '%' => "%".to_string(),
            other => format!("%{other}"),
        };
        out.push_str(&piece);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: LocalTime = LocalTime {
        year: 2026,
        month: 9,
        day: 30,
        hour: 21,
        minute: 5,
        second: 9,
        weekday: 2,
    };

    #[test]
    fn numeric_directives() {
        assert_eq!(format_clock("%H:%M:%S", &T, Language::English), "21:05:09");
        assert_eq!(format_clock("%I:%M %p", &T, Language::English), "09:05 PM");
        assert_eq!(
            format_clock("%d/%m/%Y %y %e", &T, Language::English),
            "30/09/2026 26 30"
        );
        let midnight = LocalTime { hour: 0, ..T };
        assert_eq!(format_clock("%I %p", &midnight, Language::English), "12 AM");
    }

    #[test]
    fn names_in_both_languages() {
        assert_eq!(
            format_clock("%A, %B %e", &T, Language::English),
            "Wednesday, September 30"
        );
        assert_eq!(format_clock("%a %b", &T, Language::English), "Wed Sep");
        assert_eq!(
            format_clock("%A, %e de %B", &T, Language::PortugueseBr),
            "quarta-feira, 30 de setembro"
        );
        assert_eq!(format_clock("%a %b", &T, Language::PortugueseBr), "qua set");
    }

    #[test]
    fn portuguese_locales_pick_portuguese() {
        assert_eq!(language_of(Some("pt-BR")), Language::PortugueseBr);
        assert_eq!(language_of(Some("PT_pt")), Language::PortugueseBr);
        assert_eq!(language_of(Some("en-US")), Language::English);
        assert_eq!(language_of(None), Language::English);
    }

    #[test]
    fn escapes_and_unknown_directives() {
        assert_eq!(
            format_clock("100%% %q %", &T, Language::English),
            "100% %q %"
        );
        let odd = LocalTime {
            month: 0,
            weekday: 9,
            ..T
        };
        assert_eq!(format_clock("%b %a", &odd, Language::English), "Jan Sun");
    }
}
