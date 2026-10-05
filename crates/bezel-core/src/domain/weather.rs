//! Weather configuration, location keys and localised condition names.
use super::clock::Language;
use super::sensor::{Reading, SensorKey, Snapshot};

/// A portable weather object. Coordinates are selected by city search.
#[derive(Debug, Clone, PartialEq)]
pub struct Weather {
    /// Displayed location name.
    pub city: String,
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees.
    pub longitude: f64,
    /// None follows the system language.
    pub language: Option<Language>,
    /// Display Fahrenheit instead of Celsius.
    pub fahrenheit: bool,
    /// Show the condition icon beside the text.
    pub show_icon: bool,
}

/// Valid coordinates and a bounded city label.
pub fn valid(city: &str, latitude: f64, longitude: f64) -> bool {
    !city.trim().is_empty()
        && city.chars().count() <= 160
        && latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
}

impl Weather {
    /// Location-specific temperature and condition keys.
    pub fn keys(&self) -> [SensorKey; 2] {
        SensorKey::weather(self.latitude, self.longitude)
    }
    /// City, temperature and conditions; missing data is explicit.
    pub fn text(&self, snapshot: &Snapshot, system: Language) -> String {
        let [temperature, code] = self.keys();
        let language = self.language.unwrap_or(system);
        let (Reading::Value(t), Reading::Value(c)) =
            (snapshot.get(&temperature), snapshot.get(&code))
        else {
            let unavailable = match language {
                Language::Italian => "Meteo non disponibile",
                Language::PortugueseBr => "Clima indisponível",
                Language::English => "Weather unavailable",
            };
            return format!("{}\n{unavailable}", self.city);
        };
        let (value, unit) = if self.fahrenheit {
            (t * 1.8 + 32.0, "F")
        } else {
            (t, "C")
        };
        format!(
            "{}\n{value:.0}\u{00b0}{unit}\n{}",
            self.city,
            condition(c as u16, language)
        )
    }
}

/// Condition family used by vector icons and labels.
pub fn family(code: u16) -> usize {
    match code {
        0 => 0,
        1 | 2 => 1,
        3 => 2,
        45 | 48 => 3,
        51..=67 | 80..=82 => 4,
        71..=77 | 85 | 86 => 5,
        95..=99 => 6,
        _ => 7,
    }
}

/// WMO weather-code description.
pub fn condition(code: u16, language: Language) -> &'static str {
    let labels = match language {
        Language::Italian => [
            "Sereno",
            "Parzialmente nuvoloso",
            "Nuvoloso",
            "Nebbia",
            "Pioggia",
            "Neve",
            "Temporale",
            "Non disponibile",
        ],
        Language::English => [
            "Clear",
            "Partly cloudy",
            "Cloudy",
            "Fog",
            "Rain",
            "Snow",
            "Thunderstorm",
            "Unavailable",
        ],
        Language::PortugueseBr => [
            "Limpo",
            "Parcialmente nublado",
            "Nublado",
            "Nevoeiro",
            "Chuva",
            "Neve",
            "Trovoada",
            "Indisponível",
        ],
    };
    labels[family(code)]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coordinates_and_conditions() {
        assert!(!valid("Roma", f64::NAN, 12.0));
        assert!(!valid("", 41.0, 12.0));
        assert!(!valid("Roma", 91.0, 12.0));
        assert!(valid("Roma", 41.9, 12.5));
        assert_eq!(condition(95, Language::Italian), "Temporale");
        assert_eq!(family(999), 7);
    }
    #[test]
    fn unavailable_and_temperature_conversion() {
        let w = Weather {
            city: "Roma".into(),
            latitude: 41.9,
            longitude: 12.5,
            language: Some(Language::Italian),
            fahrenheit: true,
            show_icon: true,
        };
        let mut snapshot = Snapshot::default();
        assert!(
            w.text(&snapshot, Language::English)
                .contains("Meteo non disponibile")
        );
        let [t, c] = w.keys();
        snapshot.insert(t, Reading::Value(20.0));
        snapshot.insert(c, Reading::Value(3.0));
        assert_eq!(
            w.text(&snapshot, Language::English),
            "Roma\n68\u{00b0}F\nNuvoloso"
        );
    }
}
