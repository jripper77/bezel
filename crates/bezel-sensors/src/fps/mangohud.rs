//! Game frame rates from MangoHud's CSV logs, Linux.
//!
//! MangoHud, the Vulkan/OpenGL overlay, logs while logging is on
//! (`Shift_L+F2` by default, or `autostart_log=N` in `MangoHud.conf`): one
//! CSV per session in its `output_folder`, named
//! `<program>_YYYY-MM-DD_HH-MM-SS.csv`, with a header and then one row per
//! logged interval, each flushed as it is written. `stop_logging` adds
//! `<same name>_summary.csv`, which is not a frame log (MangoHud's
//! `src/logging.cpp`; docs/reverse-engineering/sensors.md § 8). Bezel reads
//! the last complete row of the newest log; the file's modification time
//! says how old that row is.
//!
//! The golden logs of `fixtures/mangohud/` follow that format; none was
//! captured from a real game (`HARDWARE_VALIDATED` is false).

use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::{FrameRateSource, MAX_AGE};

/// How to make MangoHud log, ending every reason given while it does not.
const HOW_TO_LOG: &str = "run the game with MangoHud and start logging \
     (Shift_L+F2, or autostart_log=1 in MangoHud.conf)";

/// Why there is no reading while the newest log has no row yet.
const NO_ROW: &str = "MangoHud has not logged a frame yet";

/// Bytes read at each end of a log: the header is in the first, the last
/// complete row in the last (a row is about 100 bytes).
const END_BYTES: u64 = 4096;

/// `gpu.fps` from the newest MangoHud log in a folder.
pub(crate) struct MangoHud {
    dir: Option<PathBuf>,
}

impl MangoHud {
    /// Reads the logs in `dir`; `None` when no folder is known.
    pub(crate) fn new(dir: Option<PathBuf>) -> Self {
        Self { dir }
    }
}

impl FrameRateSource for MangoHud {
    fn describe(&self) -> String {
        match &self.dir {
            Some(dir) => format!("MangoHud logs in {}", dir.display()),
            None => "MangoHud logs (folder unknown)".to_string(),
        }
    }

    fn read(&mut self) -> Result<f64, String> {
        let dir = self.dir.as_deref().ok_or_else(|| {
            format!(
                "MangoHud's log folder is unknown (no HOME): pass --mangohud-dir, and {HOW_TO_LOG}"
            )
        })?;
        let (path, modified) = newest_log(dir)?;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let age = SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default();
        if age > MAX_AGE {
            return Err(format!(
                "MangoHud's newest log ({name}) was last written {:.1} s ago: {HOW_TO_LOG}",
                age.as_secs_f64()
            ));
        }
        let (head, tail) = read_ends(&path).map_err(|e| format!("cannot read {name}: {e}"))?;
        last_frame_rate(&head, &tail).map_err(|why| format!("{name}: {why}"))
    }
}

/// The newest frame log in `dir` and when it was last written.
fn newest_log(dir: &Path) -> Result<(PathBuf, SystemTime), String> {
    let entries = fs::read_dir(dir).map_err(|e| {
        format!(
            "cannot read MangoHud's log folder {} ({e}): set output_folder in MangoHud.conf \
             or pass --mangohud-dir, and {HOW_TO_LOG}",
            dir.display()
        )
    })?;
    entries
        .filter_map(Result::ok)
        .filter(|entry| is_log_name(&entry.file_name()))
        .filter_map(|entry| Some((entry.path(), entry.metadata().ok()?.modified().ok()?)))
        .max_by_key(|(_, modified)| *modified)
        .ok_or_else(|| format!("no MangoHud log in {}: {HOW_TO_LOG}", dir.display()))
}

/// `<program>_YYYY-MM-DD_HH-MM-SS.csv`, as MangoHud names a log (its
/// `_summary.csv` does not match).
fn is_log_name(name: &OsStr) -> bool {
    const STAMP: &[u8] = b"_0000-00-00_00-00-00";
    let Some(stem) = name.to_str().and_then(|n| n.strip_suffix(".csv")) else {
        return false;
    };
    let Some(stamp) = stem
        .len()
        .checked_sub(STAMP.len())
        .and_then(|start| stem.get(start..))
    else {
        return false;
    };
    stamp.bytes().zip(STAMP).all(|(c, p)| {
        if *p == b'0' {
            c.is_ascii_digit()
        } else {
            c == *p
        }
    })
}

/// The first and the last [`END_BYTES`] of `path` (the whole file when it
/// is shorter), as text.
fn read_ends(path: &Path) -> std::io::Result<(String, String)> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let mut head = Vec::new();
    (&mut file).take(END_BYTES).read_to_end(&mut head)?;
    let mut tail = Vec::new();
    file.seek(SeekFrom::Start(len.saturating_sub(END_BYTES)))?;
    file.take(END_BYTES).read_to_end(&mut tail)?;
    Ok((
        String::from_utf8_lossy(&head).into_owned(),
        String::from_utf8_lossy(&tail).into_owned(),
    ))
}

/// The `fps` of the last complete row, from the start of a log (with its
/// column header) and its end. A row still being written is skipped.
fn last_frame_rate(head: &str, tail: &str) -> Result<f64, String> {
    let column = head
        .lines()
        .find_map(fps_column)
        .ok_or("not a MangoHud frame log (no fps column)")?;
    let complete = tail.rfind('\n').map_or("", |end| &tail[..end]);
    let row = complete.rsplit('\n').next().unwrap_or_default();
    match row.split(',').nth(column).map(|f| f.trim().parse::<f64>()) {
        Some(Ok(fps)) if fps.is_finite() && fps >= 0.0 => Ok(fps),
        _ => Err(NO_ROW.to_string()),
    }
}

/// Where `fps` is in MangoHud's frame-metric header (`fps,frametime,…`).
fn fps_column(line: &str) -> Option<usize> {
    let columns: Vec<&str> = line.split(',').map(str::trim).collect();
    if !columns.contains(&"frametime") {
        return None;
    }
    columns.iter().position(|c| *c == "fps")
}

/// MangoHud's `output_folder` found the way MangoHud finds it (per-game
/// files aside): the first config file that exists, then its
/// `output_folder`, else the home folder, MangoHud's default.
pub(crate) fn default_dir() -> Option<PathBuf> {
    let env = |name: &str| std::env::var_os(name);
    output_folder(
        &config_files(&env, Path::new("/etc/MangoHud.conf")),
        env("HOME").map(PathBuf::from),
    )
}

/// The config files MangoHud reads, in its order: `MANGOHUD_CONFIGFILE`
/// alone when set, else `etc` (`/etc/MangoHud.conf`) and then
/// `MangoHud/MangoHud.conf` under `XDG_CONFIG_HOME` (`~/.config`).
fn config_files(env: &dyn Fn(&str) -> Option<OsString>, etc: &Path) -> Vec<PathBuf> {
    if let Some(file) = env("MANGOHUD_CONFIGFILE") {
        return vec![PathBuf::from(file)];
    }
    let config_home = env("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env("HOME").map(|home| PathBuf::from(home).join(".config")));
    let mut files = vec![etc.to_path_buf()];
    files.extend(config_home.map(|dir| dir.join("MangoHud").join("MangoHud.conf")));
    files
}

/// The `output_folder` of the first readable file of `files` (`~`
/// expanded), else `home`.
fn output_folder(files: &[PathBuf], home: Option<PathBuf>) -> Option<PathBuf> {
    let folder = files
        .iter()
        .find_map(|file| fs::read_to_string(file).ok())
        .and_then(|text| setting(&text, "output_folder"));
    let Some(folder) = folder else {
        return home;
    };
    match folder.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            Some(home?.join(rest.trim_start_matches('/')))
        }
        _ => Some(PathBuf::from(folder)),
    }
}

/// The non-empty value of `key` in a MangoHud config (`key=value` lines,
/// `#` comments, the last one wins).
fn setting(config: &str, key: &str) -> Option<String> {
    config
        .lines()
        .filter_map(|line| line.split('#').next()?.split_once('='))
        .filter(|(name, _)| name.trim() == key)
        .map(|(_, value)| value.trim().to_string())
        .next_back()
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeTree;
    use std::time::Duration;

    const WITCHER: (&str, &str) = (
        "witcher3_2026-09-30_21-05-00.csv",
        include_str!("fixtures/mangohud/witcher3_2026-09-30_21-05-00.csv"),
    );
    const WITCHER_SUMMARY: (&str, &str) = (
        "witcher3_2026-09-30_21-05-00_summary.csv",
        include_str!("fixtures/mangohud/witcher3_2026-09-30_21-05-00_summary.csv"),
    );
    const VERSIONED: (&str, &str) = (
        "witcher3_2026-09-30_22-10-00.csv",
        include_str!("fixtures/mangohud/witcher3_2026-09-30_22-10-00.csv"),
    );
    const LOADING: (&str, &str) = (
        "loading_2026-09-30_21-07-00.csv",
        include_str!("fixtures/mangohud/loading_2026-09-30_21-07-00.csv"),
    );
    const STARTED: (&str, &str) = (
        "started_2026-09-30_21-08-00.csv",
        include_str!("fixtures/mangohud/started_2026-09-30_21-08-00.csv"),
    );

    /// A log folder with `logs` written just now.
    fn folder(logs: &[(&str, &str)]) -> FakeTree {
        let tree = FakeTree::new("mangohud");
        for (name, text) in logs {
            tree.file(name, text);
        }
        tree
    }

    /// Makes `name` last written `ago`.
    fn written(tree: &FakeTree, name: &str, ago: Duration) {
        File::options()
            .write(true)
            .open(tree.path(name))
            .unwrap()
            .set_modified(SystemTime::now() - ago)
            .unwrap();
    }

    fn read(tree: &FakeTree) -> Result<f64, String> {
        MangoHud::new(Some(tree.root().to_path_buf())).read()
    }

    #[test]
    fn the_newest_log_gives_its_last_row() {
        let tree = folder(&[LOADING, WITCHER, WITCHER_SUMMARY]);
        written(&tree, LOADING.0, Duration::from_secs(60));
        written(&tree, WITCHER.0, Duration::from_secs(1));
        // The summary is newer, but it is not a frame log.
        assert_eq!(read(&tree), Ok(139.874));
    }

    #[test]
    fn versioned_logs_read_like_plain_ones() {
        assert_eq!(read(&folder(&[VERSIONED])), Ok(117.93));
    }

    #[test]
    fn a_live_zero_is_zero() {
        assert_eq!(read(&folder(&[LOADING])), Ok(0.0));
    }

    #[test]
    fn a_log_older_than_3_s_is_unavailable_never_its_last_row() {
        let tree = folder(&[WITCHER]);
        written(&tree, WITCHER.0, Duration::from_secs(4));
        let why = read(&tree).unwrap_err();
        assert!(
            why.starts_with(
                "MangoHud's newest log (witcher3_2026-09-30_21-05-00.csv) was last written 4."
            ),
            "{why}"
        );
        assert!(why.contains("Shift_L+F2"), "{why}");
        written(&tree, WITCHER.0, Duration::from_millis(2500));
        assert_eq!(read(&tree), Ok(139.874));
    }

    #[test]
    fn no_folder_or_no_log_says_how_to_enable_logging() {
        let tree = folder(&[WITCHER_SUMMARY]);
        let why = read(&tree).unwrap_err();
        assert!(why.starts_with("no MangoHud log in "), "{why}");
        assert!(why.ends_with(HOW_TO_LOG), "{why}");
        let missing = MangoHud::new(Some(tree.path("gone"))).read().unwrap_err();
        assert!(
            missing.contains("cannot read MangoHud's log folder"),
            "{missing}"
        );
        assert!(missing.contains("output_folder") && missing.contains("--mangohud-dir"));
        let unknown = MangoHud::new(None);
        assert_eq!(unknown.describe(), "MangoHud logs (folder unknown)");
        assert!(
            MangoHud::new(None)
                .read()
                .unwrap_err()
                .contains("--mangohud-dir")
        );
    }

    #[test]
    fn a_log_without_rows_yet_is_unavailable() {
        let why = read(&folder(&[STARTED])).unwrap_err();
        assert_eq!(why, format!("{}: {NO_ROW}", STARTED.0));
        let other = read(&folder(&[("x_2026-09-30_21-05-00.csv", "a,b\n1,2\n")])).unwrap_err();
        assert!(
            other.ends_with("not a MangoHud frame log (no fps column)"),
            "{other}"
        );
    }

    #[test]
    fn a_row_being_written_is_skipped() {
        let (name, text) = WITCHER;
        let tree = folder(&[(name, &format!("{text}150.2,6.6"))]);
        assert_eq!(read(&tree), Ok(139.874));
    }

    #[test]
    fn long_logs_are_read_from_both_ends() {
        let header = LOADING.1.lines().take(3).collect::<Vec<_>>().join("\n");
        let mut log = format!("{header}\n");
        for n in 0..2000 {
            log.push_str(&format!(
                "{}.5,16.6,10,20,30,40,50,1,2,3,4,5,6,7,8,{n}\n",
                60 + n % 3
            ));
        }
        assert!(log.len() as u64 > 20 * END_BYTES);
        let tree = folder(&[("long_2026-09-30_21-05-00.csv", &log)]);
        // Row 1999: 60 + 1999 % 3 = 61.
        assert_eq!(read(&tree), Ok(61.5));
    }

    #[test]
    fn log_names_follow_mangohud() {
        for name in [
            "witcher3_2026-09-30_21-05-00.csv",
            "wine-Cyberpunk2077.exe_2026-01-02_03-04-05.csv",
            "_2026-09-30_21-05-00.csv",
        ] {
            assert!(is_log_name(OsStr::new(name)), "{name}");
        }
        for name in [
            "witcher3_2026-09-30_21-05-00_summary.csv",
            "witcher3_2026-09-30_21-05-00.txt",
            "budget.csv",
            "witcher3_2026-09-30_21-05.csv",
            "witcher3_2026-09-30T21-05-00.csv",
            "ãã_2026-09-30_21-05-0.csv",
        ] {
            assert!(!is_log_name(OsStr::new(name)), "{name}");
        }
    }

    #[test]
    fn config_files_follow_mangohud_order() {
        let etc = Path::new("/etc/MangoHud.conf");
        let env = |vars: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                vars.iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        assert_eq!(
            config_files(
                &env(&[("MANGOHUD_CONFIGFILE", "/x.conf"), ("HOME", "/h")]),
                etc
            ),
            [PathBuf::from("/x.conf")]
        );
        assert_eq!(
            config_files(&env(&[("XDG_CONFIG_HOME", "/c"), ("HOME", "/h")]), etc),
            [
                etc.to_path_buf(),
                PathBuf::from("/c/MangoHud/MangoHud.conf")
            ]
        );
        assert_eq!(
            config_files(&env(&[("HOME", "/h")]), etc),
            [
                etc.to_path_buf(),
                PathBuf::from("/h/.config/MangoHud/MangoHud.conf")
            ]
        );
        assert_eq!(config_files(&env(&[]), etc), [etc.to_path_buf()]);
    }

    #[test]
    fn the_folder_is_mangohud_output_folder_else_home() {
        let tree = FakeTree::new("mangohud-conf");
        tree.file(
            "user.conf",
            "fps\noutput_folder = ~/logs # games\nfps_limit=0\n",
        )
        .file("etc.conf", "output_folder=/var/log/mh\n")
        .file("plain.conf", "fps\n")
        .file("empty.conf", "output_folder=\n")
        .file(
            "twice.conf",
            "output_folder=/a\n#output_folder=/b\noutput_folder=/c\n",
        );
        let home = Some(PathBuf::from("/home/u"));
        let at = |names: &[&str]| -> Vec<PathBuf> { names.iter().map(|n| tree.path(n)).collect() };
        let pick = |names: &[&str], home: Option<PathBuf>| output_folder(&at(names), home);
        assert_eq!(
            pick(&["gone", "user.conf"], home.clone()),
            Some("/home/u/logs".into())
        );
        // The first file that exists wins, even without output_folder.
        assert_eq!(
            pick(&["etc.conf", "user.conf"], home.clone()),
            Some("/var/log/mh".into())
        );
        assert_eq!(pick(&["plain.conf", "etc.conf"], home.clone()), home);
        assert_eq!(pick(&["empty.conf"], home.clone()), home);
        assert_eq!(pick(&["twice.conf"], home.clone()), Some("/c".into()));
        assert_eq!(pick(&["gone"], home.clone()), home);
        assert_eq!(pick(&["gone"], None), None);
        assert_eq!(pick(&["user.conf"], None), None);
        tree.file("tilde.conf", "output_folder=~\n");
        assert_eq!(pick(&["tilde.conf"], home.clone()), home);
        tree.file("user2.conf", "output_folder=~user/logs\n");
        assert_eq!(pick(&["user2.conf"], home), Some("~user/logs".into()));
    }
}
