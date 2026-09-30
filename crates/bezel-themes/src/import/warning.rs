//! What an import could not map exactly, as a stable code with named
//! arguments (D-2026-09-30-release-polish-6): a UI translates the code with
//! the arguments; the CLI prints the English sentence (`Display`).

use std::fmt;

/// Defines [`WarningCode`]: each variant with its stable code and its English
/// sentence, where `{name}` stands for the argument `name`. [`WarningCode::ALL`]
/// lists every variant, so nothing can be left out of it.
macro_rules! warning_codes {
    ($($variant:ident = $code:literal => $english:literal,)+) => {
        /// A kind of thing an import could not map exactly.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum WarningCode {
            $(#[doc = $english] $variant,)+
        }

        impl WarningCode {
            /// Every code, in declaration order.
            pub const ALL: &'static [WarningCode] = &[$(WarningCode::$variant,)+];

            /// The stable name a UI translates.
            pub const fn code(self) -> &'static str {
                match self {
                    $(WarningCode::$variant => $code,)+
                }
            }

            /// The English sentence; `{name}` stands for the argument `name`.
            pub const fn english(self) -> &'static str {
                match self {
                    $(WarningCode::$variant => $english,)+
                }
            }
        }
    };
}

warning_codes! {
    // ------------------------------------------------ TURZX (.turtheme) --
    NoLayers = "noLayers" => "the theme has no layers",
    VideoWithoutName = "videoWithoutName"
        => "the background video has no file name; only its poster was imported",
    VideoNotFound = "videoNotFound"
        => "the background video {name} is not inside the .turtheme and was not found next \
            to it; copy it into the theme as {path}",
    LayerWithoutPicture = "layerWithoutPicture" => "a {layer} layer without a picture was dropped",
    UnsupportedPicture = "unsupportedPicture"
        => "a {layer} picture that is not PNG, GIF or JPEG was dropped",
    VideoLayer = "videoLayer"
        => "a video layer above the background is not supported; its poster is shown",
    UnknownLayer = "unknownLayer" => "a layer of unknown type \"{type}\" was dropped",
    TextWithoutFont = "textWithoutFont" => "a text layer without font settings was dropped",
    ChineseMonths = "chineseMonths" => "Chinese month names are shown as numbers",
    WeekdayFormat = "weekdayFormat"
        => "the weekday format {format} is shown as an English short name",
    UnknownClockFormat = "unknownClockFormat"
        => "the {field} format \"{format}\" is unknown; a default is used",
    UnknownDataSource = "unknownDataSource"
        => "the data source \"{source}\" is unknown; it was kept as vendor.{source}",
    GpuFanPercent = "gpuFanPercent"
        => "{source}: the vendor app shows the GPU fan in RPM; Bezel shows its duty in percent",
    MemoryModel = "memoryModel" => "{source}: the memory \"model\" text has no Bezel equivalent",
    RebindSensor = "rebindSensor"
        => "{source}: the vendor app lets the user pick this sensor; rebind it in the editor",
    DriveLetters = "driveLetters"
        => "{source}: Windows drive letters were kept; rebind the layer to a mount point",
    SensorNotSupported = "sensorNotSupported"
        => "{source}: not supported yet; the layer shows it as unavailable",
    VendorWeather = "vendorWeather" => "{source}: the vendor weather service is not supported",
    UnsupportedBinding = "unsupportedBinding"
        => "bars, rings, needles and charts bound to {source} are not supported; bound to \
            vendor.{source}",
    SystemColors = "systemColors"
        => "Windows system colors were replaced by their Windows 10 defaults",
    UnknownKnownColor = "unknownKnownColor" => "the .NET known color {index} is unknown",
    UnknownColorName = "unknownColorName" => "the color name \"{color}\" is unknown",
    FontsNotStored = "fontsNotStored"
        => "fonts are not stored in .turtheme files; install them or pick others: {fonts}",
    LayerWithoutData = "layerWithoutData" => "a {layer} without a data source was dropped",
    InvertedBar = "invertedBar"
        => "inverted bars (1 - value) are not supported; they fill normally",
    ChartDirection = "chartDirection"
        => "charts scrolling from right to left are not supported; they scroll normally",
    // ------------------------------- turing-smart-screen-python (.yaml) --
    UnusedTopKey = "unusedTopKey" => "the top-level key {key} is not used",
    NotAColor = "notAColor" => "{name}: the {key} \"{value}\" is not a color; a default is used",
    UnusedKey = "unusedKey" => "{name}: {key} is not used",
    UnknownDisplaySize = "unknownDisplaySize" => "the display size {size} is unknown; 3.5\" is used",
    UnknownOrientation = "unknownOrientation"
        => "the display orientation \"{orientation}\" is unknown; portrait is used",
    BackplateLed = "backplateLed"
        => "the backplate LED color (XuanFang rev B) is not part of a Bezel theme",
    NoPath = "noPath" => "{name}: no PATH; dropped",
    PathOutside = "pathOutside" => "{name}: the path \"{path}\" leaves the theme folder; dropped",
    FileUnreadable = "fileUnreadable" => "{name}: {error}; dropped",
    NotAnImage = "notAnImage" => "{name}: {path} is not a PNG, GIF or JPEG; dropped",
    FontPathOutside = "fontPathOutside"
        => "the font path \"{path}\" leaves the fonts folder; not bundled",
    NoFontsFolder = "noFontsFolder"
        => "the Python fonts folder (res/fonts) was not found; fonts are not bundled",
    FontNotBundled = "fontNotBundled" => "the font {path} was not bundled: {error}",
    NoText = "noText" => "{name}: no TEXT; dropped",
    UnknownSensor = "unknownSensor" => "STATS.{name}: unknown sensor; dropped",
    CpuFanGuessed = "cpuFanGuessed"
        => "STATS.{name}: the Python app estimates this percent from the fan's RPM; Bezel \
            measures the RPM (cpu.fan): rebind the widget to it and set its range",
    WeatherNotSupported = "weatherNotSupported" => "STATS.{name}: weather is not supported yet",
    CustomData = "customData" => "STATS.{name}: custom Python data classes cannot run in Bezel",
    DateField = "dateField"
        => "the date/time field {field} of \"{format}\" is not supported and was left out",
    BarWithoutSize = "barWithoutSize" => "STATS.{name}: a bar without a size was dropped",
    RadialWithoutRadius = "radialWithoutRadius"
        => "STATS.{name}: a radial bar without a radius was dropped",
    UnknownDecoration = "unknownDecoration"
        => "STATS.{name}: the bar decoration \"{decoration}\" is unknown",
    GraphAxes = "graphAxes" => "STATS.{name}: line graph axes and labels are not supported",
}

/// The values of the `layer` argument: kinds of vendor layers, which a UI
/// translates too.
pub const LAYER_NAMES: [&str; 7] = [
    "background",
    "bar",
    "chart",
    "image",
    "needle",
    "poster",
    "ring",
];

impl WarningCode {
    /// The names of the arguments the sentence takes, in order of first use.
    pub fn params(self) -> Vec<&'static str> {
        let mut names = Vec::new();
        let mut rest = self.english();
        while let Some(start) = rest.find('{') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('}') else {
                break;
            };
            let name = &after[..end];
            if !names.contains(&name) {
                names.push(name);
            }
            rest = &after[end + 1..];
        }
        names
    }
}

/// One thing an import could not map exactly: a [`WarningCode`] and the
/// values of its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportWarning {
    code: WarningCode,
    args: Vec<(&'static str, String)>,
}

impl ImportWarning {
    /// A warning of `code`; add each argument its sentence takes with
    /// [`Self::arg`].
    pub fn new(code: WarningCode) -> Self {
        Self {
            code,
            args: Vec::new(),
        }
    }

    /// Sets the argument `name` (one of the code's [`WarningCode::params`]).
    #[must_use]
    pub fn arg(mut self, name: &'static str, value: impl Into<String>) -> Self {
        let value = value.into();
        debug_assert!(
            self.code.params().contains(&name),
            "{name} is not an argument of {}",
            self.code.code()
        );
        debug_assert!(
            name != "layer" || LAYER_NAMES.contains(&value.as_str()),
            "unknown layer {value}"
        );
        self.args.retain(|(n, _)| *n != name);
        self.args.push((name, value));
        self
    }

    /// What kind of warning it is.
    pub fn code(&self) -> WarningCode {
        self.code
    }

    /// The arguments, by name.
    pub fn args(&self) -> &[(&'static str, String)] {
        &self.args
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.args
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A warning without arguments.
impl From<WarningCode> for ImportWarning {
    fn from(code: WarningCode) -> Self {
        Self::new(code)
    }
}

/// The English sentence, with the arguments filled in (a missing one stays
/// `{name}`).
impl fmt::Display for ImportWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut rest = self.code.english();
        while let Some(start) = rest.find('{') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('}') else {
                break;
            };
            f.write_str(&rest[..start])?;
            let name = &after[..end];
            match self.value(name) {
                Some(value) => f.write_str(value)?,
                None => write!(f, "{{{name}}}")?,
            }
            rest = &after[end + 1..];
        }
        f.write_str(rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn codes_are_unique_camel_case_names() {
        let codes: BTreeSet<&str> = WarningCode::ALL.iter().map(|c| c.code()).collect();
        assert_eq!(codes.len(), WarningCode::ALL.len());
        for code in codes {
            assert!(
                code.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                    && code.chars().all(|c| c.is_ascii_alphanumeric()),
                "{code}"
            );
        }
    }

    #[test]
    fn sentences_name_their_arguments() {
        assert_eq!(
            WarningCode::VideoNotFound.params(),
            vec!["name", "path"],
            "in order of first use"
        );
        assert_eq!(
            WarningCode::UnknownDataSource.params(),
            vec!["source"],
            "once each"
        );
        assert!(WarningCode::NoLayers.params().is_empty());
        for code in WarningCode::ALL {
            for name in code.params() {
                assert!(
                    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric()),
                    "{}: {{{name}}}",
                    code.code()
                );
            }
        }
    }

    #[test]
    fn the_english_sentence_fills_the_arguments() {
        let w = ImportWarning::new(WarningCode::VideoNotFound)
            .arg("name", "AMD.mp4")
            .arg("path", "assets/amd.mp4");
        assert_eq!(
            w.to_string(),
            "the background video AMD.mp4 is not inside the .turtheme and was not found next to \
             it; copy it into the theme as assets/amd.mp4"
        );
        assert_eq!(w.code(), WarningCode::VideoNotFound);
        assert_eq!(w.args()[1], ("path", "assets/amd.mp4".to_string()));
        let again = ImportWarning::new(WarningCode::UnknownKnownColor)
            .arg("index", "1")
            .arg("index", "2");
        assert_eq!(
            again.args().len(),
            1,
            "an argument set twice keeps the last"
        );
        assert_eq!(again.to_string(), "the .NET known color 2 is unknown");
        assert_eq!(
            ImportWarning::new(WarningCode::NoText).to_string(),
            "{name}: no TEXT; dropped",
            "a missing argument shows its name"
        );
    }
}
