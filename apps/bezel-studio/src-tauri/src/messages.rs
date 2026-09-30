//! Messages the backend sends to the UI as a stable code with named
//! arguments (D-2026-09-30-release-polish-6): the UI translates the code and
//! fills in the arguments; `message` is the English text, for logs and for a
//! code the UI does not know. `tests/ui/fixtures/backend-codes.json` lists
//! every code with its arguments; a test here keeps it in step and the UI's
//! tests check that both languages translate each one.

use std::collections::BTreeMap;

use bezel_themes::import::ImportWarning;
use serde::Serialize;

/// One import warning as the UI gets it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WarningDto {
    /// The warning's code (`bezel_themes::import::WarningCode`).
    pub code: &'static str,
    /// Its arguments, by name.
    pub args: BTreeMap<&'static str, String>,
    /// The English sentence.
    pub message: String,
}

impl From<&ImportWarning> for WarningDto {
    fn from(w: &ImportWarning) -> Self {
        Self {
            code: w.code().code(),
            args: w.args().iter().cloned().collect(),
            message: w.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_themes::import::{LAYER_NAMES, WarningCode};
    use serde_json::{Value, json};

    /// The fixture the UI's tests read.
    const FIXTURE: &str = include_str!("../../tests/ui/fixtures/backend-codes.json");

    fn sorted(mut names: Vec<&str>) -> Vec<&str> {
        names.sort_unstable();
        names
    }

    /// The part of the fixture this crate's codes determine.
    fn expected() -> Value {
        let warnings: BTreeMap<&str, Vec<&str>> = WarningCode::ALL
            .iter()
            .map(|c| (c.code(), sorted(c.params())))
            .collect();
        let mut layers = LAYER_NAMES.to_vec();
        layers.sort_unstable();
        json!({
            "importLayers": layers,
            "importWarnings": warnings,
        })
    }

    #[test]
    fn the_ui_fixture_lists_every_code_with_its_arguments() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let expected = expected();
        assert_eq!(
            fixture,
            expected,
            "tests/ui/fixtures/backend-codes.json is out of date; it should read:\n{}",
            serde_json::to_string_pretty(&expected).unwrap()
        );
    }

    #[test]
    fn warnings_carry_code_arguments_and_english() {
        let w = ImportWarning::new(WarningCode::UnknownColorName).arg("color", "Nope");
        let json = serde_json::to_value(WarningDto::from(&w)).unwrap();
        assert_eq!(
            json,
            json!({
                "code": "unknownColorName",
                "args": {"color": "Nope"},
                "message": "the color name \"Nope\" is unknown",
            })
        );
    }
}
