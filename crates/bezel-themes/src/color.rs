//! `#rrggbb` / `#rrggbbaa` color text.

use bezel_core::domain::frame::Rgba;

/// `#rrggbbaa` (alpha always written).
pub fn to_hex(c: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
}

/// Parses `#rgb`, `#rrggbb` or `#rrggbbaa` (case-insensitive).
pub fn from_hex(s: &str) -> Option<Rgba> {
    let hex = s.trim().strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        3 => {
            let nib = |i: usize| u8::from_str_radix(hex.get(i..=i)?, 16).ok().map(|v| v * 17);
            Some(Rgba::opaque(nib(0)?, nib(1)?, nib(2)?))
        }
        6 => Some(Rgba::opaque(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Rgba {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: byte(6)?,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip_and_short_forms() {
        let c = Rgba {
            r: 0x12,
            g: 0xab,
            b: 0xff,
            a: 0x80,
        };
        assert_eq!(to_hex(c), "#12abff80");
        assert_eq!(from_hex("#12ABFF80"), Some(c));
        assert_eq!(from_hex("#fff"), Some(Rgba::WHITE));
        assert_eq!(from_hex(" #000000 "), Some(Rgba::BLACK));
        for bad in ["", "fff", "#ff", "#gggggg", "#12345", "#ééé"] {
            assert_eq!(from_hex(bad), None, "{bad}");
        }
    }
}
