//! Named colors shared by the importers: the .NET `KnownColor` table (used
//! by `System.Drawing.Color` in vendor themes) and the CSS/X11 names Pillow
//! accepts in Python themes (the same web colors).

use bezel_core::domain::frame::Rgba;

/// `.NET` system colors (`KnownColor` 1..=26 and 168..=174). Their real value
/// depends on the Windows theme; these are the Windows 10 defaults.
const SYSTEM: [(u16, &str, u32); 33] = [
    (1, "ActiveBorder", 0xFFB4B4B4),
    (2, "ActiveCaption", 0xFF99B4D1),
    (3, "ActiveCaptionText", 0xFF000000),
    (4, "AppWorkspace", 0xFFABABAB),
    (5, "Control", 0xFFF0F0F0),
    (6, "ControlDark", 0xFFA0A0A0),
    (7, "ControlDarkDark", 0xFF696969),
    (8, "ControlLight", 0xFFE3E3E3),
    (9, "ControlLightLight", 0xFFFFFFFF),
    (10, "ControlText", 0xFF000000),
    (11, "Desktop", 0xFF000000),
    (12, "GrayText", 0xFF6D6D6D),
    (13, "Highlight", 0xFF0078D7),
    (14, "HighlightText", 0xFFFFFFFF),
    (15, "HotTrack", 0xFF0066CC),
    (16, "InactiveBorder", 0xFFF4F7FC),
    (17, "InactiveCaption", 0xFFBFCDDB),
    (18, "InactiveCaptionText", 0xFF000000),
    (19, "Info", 0xFFFFFFE1),
    (20, "InfoText", 0xFF000000),
    (21, "Menu", 0xFFF0F0F0),
    (22, "MenuText", 0xFF000000),
    (23, "ScrollBar", 0xFFC8C8C8),
    (24, "Window", 0xFFFFFFFF),
    (25, "WindowFrame", 0xFF646464),
    (26, "WindowText", 0xFF000000),
    (168, "ButtonFace", 0xFFF0F0F0),
    (169, "ButtonHighlight", 0xFFFFFFFF),
    (170, "ButtonShadow", 0xFFA0A0A0),
    (171, "GradientActiveCaption", 0xFFB9D1EA),
    (172, "GradientInactiveCaption", 0xFFD7E4F2),
    (173, "MenuBar", 0xFFF0F0F0),
    (174, "MenuHighlight", 0xFF3399FF),
];

/// Web colors in `KnownColor` order, starting at 27 (`Transparent`).
const WEB: [(&str, u32); 141] = [
    ("Transparent", 0x00FFFFFF),
    ("AliceBlue", 0xFFF0F8FF),
    ("AntiqueWhite", 0xFFFAEBD7),
    ("Aqua", 0xFF00FFFF),
    ("Aquamarine", 0xFF7FFFD4),
    ("Azure", 0xFFF0FFFF),
    ("Beige", 0xFFF5F5DC),
    ("Bisque", 0xFFFFE4C4),
    ("Black", 0xFF000000),
    ("BlanchedAlmond", 0xFFFFEBCD),
    ("Blue", 0xFF0000FF),
    ("BlueViolet", 0xFF8A2BE2),
    ("Brown", 0xFFA52A2A),
    ("BurlyWood", 0xFFDEB887),
    ("CadetBlue", 0xFF5F9EA0),
    ("Chartreuse", 0xFF7FFF00),
    ("Chocolate", 0xFFD2691E),
    ("Coral", 0xFFFF7F50),
    ("CornflowerBlue", 0xFF6495ED),
    ("Cornsilk", 0xFFFFF8DC),
    ("Crimson", 0xFFDC143C),
    ("Cyan", 0xFF00FFFF),
    ("DarkBlue", 0xFF00008B),
    ("DarkCyan", 0xFF008B8B),
    ("DarkGoldenrod", 0xFFB8860B),
    ("DarkGray", 0xFFA9A9A9),
    ("DarkGreen", 0xFF006400),
    ("DarkKhaki", 0xFFBDB76B),
    ("DarkMagenta", 0xFF8B008B),
    ("DarkOliveGreen", 0xFF556B2F),
    ("DarkOrange", 0xFFFF8C00),
    ("DarkOrchid", 0xFF9932CC),
    ("DarkRed", 0xFF8B0000),
    ("DarkSalmon", 0xFFE9967A),
    ("DarkSeaGreen", 0xFF8FBC8B),
    ("DarkSlateBlue", 0xFF483D8B),
    ("DarkSlateGray", 0xFF2F4F4F),
    ("DarkTurquoise", 0xFF00CED1),
    ("DarkViolet", 0xFF9400D3),
    ("DeepPink", 0xFFFF1493),
    ("DeepSkyBlue", 0xFF00BFFF),
    ("DimGray", 0xFF696969),
    ("DodgerBlue", 0xFF1E90FF),
    ("Firebrick", 0xFFB22222),
    ("FloralWhite", 0xFFFFFAF0),
    ("ForestGreen", 0xFF228B22),
    ("Fuchsia", 0xFFFF00FF),
    ("Gainsboro", 0xFFDCDCDC),
    ("GhostWhite", 0xFFF8F8FF),
    ("Gold", 0xFFFFD700),
    ("Goldenrod", 0xFFDAA520),
    ("Gray", 0xFF808080),
    ("Green", 0xFF008000),
    ("GreenYellow", 0xFFADFF2F),
    ("Honeydew", 0xFFF0FFF0),
    ("HotPink", 0xFFFF69B4),
    ("IndianRed", 0xFFCD5C5C),
    ("Indigo", 0xFF4B0082),
    ("Ivory", 0xFFFFFFF0),
    ("Khaki", 0xFFF0E68C),
    ("Lavender", 0xFFE6E6FA),
    ("LavenderBlush", 0xFFFFF0F5),
    ("LawnGreen", 0xFF7CFC00),
    ("LemonChiffon", 0xFFFFFACD),
    ("LightBlue", 0xFFADD8E6),
    ("LightCoral", 0xFFF08080),
    ("LightCyan", 0xFFE0FFFF),
    ("LightGoldenrodYellow", 0xFFFAFAD2),
    ("LightGray", 0xFFD3D3D3),
    ("LightGreen", 0xFF90EE90),
    ("LightPink", 0xFFFFB6C1),
    ("LightSalmon", 0xFFFFA07A),
    ("LightSeaGreen", 0xFF20B2AA),
    ("LightSkyBlue", 0xFF87CEFA),
    ("LightSlateGray", 0xFF778899),
    ("LightSteelBlue", 0xFFB0C4DE),
    ("LightYellow", 0xFFFFFFE0),
    ("Lime", 0xFF00FF00),
    ("LimeGreen", 0xFF32CD32),
    ("Linen", 0xFFFAF0E6),
    ("Magenta", 0xFFFF00FF),
    ("Maroon", 0xFF800000),
    ("MediumAquamarine", 0xFF66CDAA),
    ("MediumBlue", 0xFF0000CD),
    ("MediumOrchid", 0xFFBA55D3),
    ("MediumPurple", 0xFF9370DB),
    ("MediumSeaGreen", 0xFF3CB371),
    ("MediumSlateBlue", 0xFF7B68EE),
    ("MediumSpringGreen", 0xFF00FA9A),
    ("MediumTurquoise", 0xFF48D1CC),
    ("MediumVioletRed", 0xFFC71585),
    ("MidnightBlue", 0xFF191970),
    ("MintCream", 0xFFF5FFFA),
    ("MistyRose", 0xFFFFE4E1),
    ("Moccasin", 0xFFFFE4B5),
    ("NavajoWhite", 0xFFFFDEAD),
    ("Navy", 0xFF000080),
    ("OldLace", 0xFFFDF5E6),
    ("Olive", 0xFF808000),
    ("OliveDrab", 0xFF6B8E23),
    ("Orange", 0xFFFFA500),
    ("OrangeRed", 0xFFFF4500),
    ("Orchid", 0xFFDA70D6),
    ("PaleGoldenrod", 0xFFEEE8AA),
    ("PaleGreen", 0xFF98FB98),
    ("PaleTurquoise", 0xFFAFEEEE),
    ("PaleVioletRed", 0xFFDB7093),
    ("PapayaWhip", 0xFFFFEFD5),
    ("PeachPuff", 0xFFFFDAB9),
    ("Peru", 0xFFCD853F),
    ("Pink", 0xFFFFC0CB),
    ("Plum", 0xFFDDA0DD),
    ("PowderBlue", 0xFFB0E0E6),
    ("Purple", 0xFF800080),
    ("Red", 0xFFFF0000),
    ("RosyBrown", 0xFFBC8F8F),
    ("RoyalBlue", 0xFF4169E1),
    ("SaddleBrown", 0xFF8B4513),
    ("Salmon", 0xFFFA8072),
    ("SandyBrown", 0xFFF4A460),
    ("SeaGreen", 0xFF2E8B57),
    ("SeaShell", 0xFFFFF5EE),
    ("Sienna", 0xFFA0522D),
    ("Silver", 0xFFC0C0C0),
    ("SkyBlue", 0xFF87CEEB),
    ("SlateBlue", 0xFF6A5ACD),
    ("SlateGray", 0xFF708090),
    ("Snow", 0xFFFFFAFA),
    ("SpringGreen", 0xFF00FF7F),
    ("SteelBlue", 0xFF4682B4),
    ("Tan", 0xFFD2B48C),
    ("Teal", 0xFF008080),
    ("Thistle", 0xFFD8BFD8),
    ("Tomato", 0xFFFF6347),
    ("Turquoise", 0xFF40E0D0),
    ("Violet", 0xFFEE82EE),
    ("Wheat", 0xFFF5DEB3),
    ("White", 0xFFFFFFFF),
    ("WhiteSmoke", 0xFFF5F5F5),
    ("Yellow", 0xFFFFFF00),
    ("YellowGreen", 0xFF9ACD32),
];

/// First `KnownColor` value of [`WEB`].
const WEB_FIRST: i64 = 27;

/// An ARGB word (`0xAARRGGBB`) as a color.
pub fn from_argb(argb: u32) -> Rgba {
    let [a, r, g, b] = argb.to_be_bytes();
    Rgba { r, g, b, a }
}

/// Where a known color comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Known {
    /// A fixed web color.
    Web(Rgba),
    /// A Windows system color, approximated with the Windows 10 default.
    System(Rgba),
}

/// The color of a .NET `KnownColor` value.
pub fn known_color(index: i64) -> Option<Known> {
    if let Some(i) = index
        .checked_sub(WEB_FIRST)
        .and_then(|i| usize::try_from(i).ok())
        && let Some((_, argb)) = WEB.get(i)
    {
        return Some(Known::Web(from_argb(*argb)));
    }
    SYSTEM
        .iter()
        .find(|(i, _, _)| i64::from(*i) == index)
        .map(|(_, _, argb)| Known::System(from_argb(*argb)))
}

/// A color by name (case-insensitive; `grey` spellings accepted), from the
/// web colors, the system colors and CSS's `rebeccapurple`.
pub fn named_color(name: &str) -> Option<Rgba> {
    let name = name.trim().to_ascii_lowercase().replace("grey", "gray");
    if name == "rebeccapurple" {
        return Some(Rgba::opaque(0x66, 0x33, 0x99));
    }
    let web = WEB.iter().map(|(n, argb)| (*n, *argb));
    let system = SYSTEM.iter().map(|(_, n, argb)| (*n, *argb));
    web.chain(system)
        .find(|(n, _)| n.eq_ignore_ascii_case(&name))
        .map(|(_, argb)| from_argb(argb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_colors_of_the_corpus() {
        let web = |i| match known_color(i) {
            Some(Known::Web(c)) => c,
            other => panic!("{i}: {other:?}"),
        };
        assert_eq!(web(35), Rgba::BLACK);
        assert_eq!(web(66), Rgba::opaque(255, 20, 147));
        assert_eq!(web(95), Rgba::opaque(211, 211, 211));
        assert_eq!(web(140), Rgba::opaque(128, 0, 128));
        assert_eq!(web(141), Rgba::opaque(255, 0, 0));
        assert_eq!(web(150), Rgba::opaque(192, 192, 192));
        assert_eq!(web(151), Rgba::opaque(135, 206, 235));
        assert_eq!(web(164), Rgba::WHITE);
        assert_eq!(web(167), Rgba::opaque(154, 205, 50));
        assert_eq!(web(27).a, 0, "Transparent");
        assert_eq!(
            known_color(13),
            Some(Known::System(Rgba::opaque(0, 120, 215)))
        );
        assert_eq!(
            known_color(174),
            Some(Known::System(Rgba::opaque(0x33, 0x99, 0xff)))
        );
        for bad in [0, -1, 175, 1000] {
            assert_eq!(known_color(bad), None, "{bad}");
        }
    }

    #[test]
    fn names_are_case_insensitive() {
        assert_eq!(named_color("red"), Some(Rgba::opaque(255, 0, 0)));
        assert_eq!(named_color(" DarkGrey "), Some(Rgba::opaque(169, 169, 169)));
        assert_eq!(named_color("windowtext"), Some(Rgba::BLACK));
        assert_eq!(
            named_color("RebeccaPurple"),
            Some(Rgba::opaque(0x66, 0x33, 0x99))
        );
        assert_eq!(named_color("not-a-color"), None);
        assert_eq!(
            from_argb(0x80102030),
            Rgba {
                r: 0x10,
                g: 0x20,
                b: 0x30,
                a: 0x80
            }
        );
    }
}
