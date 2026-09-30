//! Reading `/proc` and `/sys` files, with errors turned into the reason a
//! reading is unavailable.

use std::io;
use std::path::Path;

use bezel_core::domain::sensor::Reading;

/// The file's text without trailing whitespace.
pub(crate) fn read_text(path: &Path) -> io::Result<String> {
    let mut text = std::fs::read_to_string(path)?;
    text.truncate(text.trim_end().len());
    Ok(text)
}

/// Why `path` could not be read, in words for the user. Uses the error kind
/// (always English) rather than the OS message (localized).
pub(crate) fn read_error(path: &Path, err: &io::Error) -> String {
    match err.kind() {
        io::ErrorKind::PermissionDenied => format!("permission denied reading {}", path.display()),
        io::ErrorKind::NotFound => format!("{} does not exist", path.display()),
        kind => format!("cannot read {}: {kind}", path.display()),
    }
}

/// An integer file (sysfs attributes are one decimal number).
pub(crate) fn read_int(path: &Path) -> Result<i64, String> {
    let text = read_text(path).map_err(|e| read_error(path, &e))?;
    text.trim()
        .parse::<i64>()
        .map_err(|_| format!("unexpected content in {}", path.display()))
}

/// An integer file divided by `divisor` (sysfs units: m°C, mV, µW, kHz...).
pub(crate) fn read_scaled(path: &Path, divisor: f64) -> Reading {
    match read_int(path) {
        Ok(v) => Reading::Value(v as f64 / divisor),
        Err(reason) => Reading::Unavailable(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeTree;

    #[test]
    fn integers_scale_and_errors_explain() {
        let t = FakeTree::new("fs");
        t.file("temp", "46625\n").file("junk", "abc\n");
        assert_eq!(read_scaled(&t.path("temp"), 1000.0), Reading::Value(46.625));
        assert_eq!(read_int(&t.path("temp")), Ok(46625));
        let junk = read_int(&t.path("junk")).unwrap_err();
        assert!(junk.starts_with("unexpected content in"), "{junk}");
        let missing = read_int(&t.path("nope")).unwrap_err();
        assert!(missing.ends_with("nope does not exist"), "{missing}");
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        assert_eq!(
            read_error(Path::new("/x"), &denied),
            "permission denied reading /x"
        );
        let other = io::Error::from(io::ErrorKind::TimedOut);
        assert_eq!(
            read_error(Path::new("/x"), &other),
            "cannot read /x: timed out"
        );
    }
}
