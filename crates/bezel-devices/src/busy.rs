//! Who else holds a device node open (Linux: `/proc/<pid>/fd` links).
//!
//! Two programs writing to the same screen interleave their packets, and the
//! screen shows garbage. Before opening a port the connector asks which other
//! processes have it open, so the user is told which program to stop.

use std::fs;
use std::path::{Path, PathBuf};

/// Processes (other than `own_pid`) that have `device` open, as
/// `"<command> (PID <pid>)"`, scanning a `/proc`-like tree at `proc_root`.
/// Unreadable entries (other users' processes) are skipped.
pub fn holders_in(proc_root: &Path, device: &Path, own_pid: u32) -> Vec<String> {
    let target = fs::canonicalize(device).unwrap_or_else(|_| device.to_path_buf());
    let Ok(entries) = fs::read_dir(proc_root) else {
        return Vec::new();
    };
    let mut found: Vec<(u32, String)> = entries
        .flatten()
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            (pid != own_pid && holds(&e.path(), &target)).then(|| (pid, describe(&e.path())))
        })
        .collect();
    found.sort();
    found.dedup();
    found
        .into_iter()
        .map(|(pid, name)| format!("{name} (PID {pid})"))
        .collect()
}

fn holds(process: &Path, target: &Path) -> bool {
    let Ok(fds) = fs::read_dir(process.join("fd")) else {
        return false;
    };
    fds.flatten()
        .filter_map(|fd| fs::read_link(fd.path()).ok())
        .any(|link| same_file(&link, target))
}

fn same_file(link: &Path, target: &Path) -> bool {
    link == target
        || fs::canonicalize(link)
            .map(PathBuf::from)
            .is_ok_and(|p| p == target)
}

/// `argv[0]`'s file name plus the first argument (so `python main.py` is
/// recognisable), or the `comm` name.
fn describe(process: &Path) -> String {
    let cmdline = fs::read(process.join("cmdline")).unwrap_or_default();
    let args: Vec<String> = cmdline
        .split(|&b| b == 0)
        .filter(|a| !a.is_empty())
        .take(2)
        .map(|a| {
            let s = String::from_utf8_lossy(a);
            Path::new(s.as_ref())
                .file_name()
                .map_or_else(|| s.to_string(), |n| n.to_string_lossy().into_owned())
        })
        .collect();
    if !args.is_empty() {
        return args.join(" ");
    }
    fs::read_to_string(process.join("comm"))
        .map_or_else(|_| "unknown".into(), |c| c.trim().to_string())
}

/// The holders of `device` on this machine (empty where `/proc` does not exist).
pub fn holders(device: &str) -> Vec<String> {
    holders_in(Path::new("/proc"), Path::new(device), std::process::id())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn fake_proc(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("bezel-busy-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn process(root: &Path, pid: u32, cmdline: &[&str], comm: &str, links: &[&Path]) {
        let dir = root.join(pid.to_string());
        fs::create_dir_all(dir.join("fd")).unwrap();
        let joined: Vec<u8> = cmdline.iter().flat_map(|a| a.bytes().chain([0])).collect();
        fs::write(dir.join("cmdline"), joined).unwrap();
        fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
        for (i, l) in links.iter().enumerate() {
            symlink(l, dir.join("fd").join(i.to_string())).unwrap();
        }
    }

    #[test]
    fn finds_other_processes_holding_the_device() {
        let root = fake_proc("find");
        let device = root.join("ttyACM1");
        fs::write(&device, b"").unwrap();
        let other = root.join("other");
        fs::write(&other, b"").unwrap();
        process(
            &root,
            2472,
            &["/venv/bin/python", "/opt/app/main.py", "--x"],
            "python",
            &[&device],
        );
        process(&root, 99, &[], "TURZX", &[&device, &other]);
        process(&root, 7, &["/usr/bin/bash"], "bash", &[&other]);
        process(&root, 42, &["/usr/bin/bezel"], "bezel", &[&device]);
        fs::create_dir_all(root.join("self")).unwrap();
        let found = holders_in(&root, &device, 42);
        assert_eq!(
            found,
            vec![
                "TURZX (PID 99)".to_string(),
                "python main.py (PID 2472)".to_string()
            ]
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_proc_or_device_yields_nothing() {
        assert!(holders_in(Path::new("/no/such/proc"), Path::new("/dev/null"), 1).is_empty());
        let root = fake_proc("empty");
        assert!(holders_in(&root, &root.join("absent"), 1).is_empty());
        assert!(holders("/dev/bezel-no-such-port").is_empty());
        fs::remove_dir_all(&root).unwrap();
    }
}
