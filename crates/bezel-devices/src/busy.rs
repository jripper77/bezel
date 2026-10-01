//! Who holds a device node open (Linux: `/proc/<pid>/fd` links).
//!
//! Two programs writing to the same screen interleave their packets, and the
//! screen shows garbage. Before opening a port the connector asks which other
//! processes have it open, so the user is told which program to stop. A port
//! this very process holds is told apart: reopening it fails at once instead
//! of passing for a display shutting down (D-2026-10-01-live-screen-controls-4).

use std::fs;
use std::path::Path;

/// The processes that have a device open, each as `"<command> (PID <pid>)"`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Holders {
    /// This process, when it has the device open itself.
    pub this: Option<String>,
    /// The other processes, by PID.
    pub others: Vec<String>,
}

/// The processes that have `device` open, scanning a `/proc`-like tree at
/// `proc_root`, `own_pid` told apart from the others. Unreadable entries
/// (other users' processes) are skipped; without the tree, nobody.
pub fn holders_by_pid(proc_root: &Path, device: &Path, own_pid: u32) -> Holders {
    let target = fs::canonicalize(device).unwrap_or_else(|_| device.to_path_buf());
    let Ok(entries) = fs::read_dir(proc_root) else {
        return Holders::default();
    };
    let mut found: Vec<(u32, String)> = entries
        .flatten()
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            holds(&e.path(), &target).then(|| (pid, describe(&e.path())))
        })
        .collect();
    found.sort();
    found.dedup();
    let mut holders = Holders::default();
    for (pid, name) in found {
        let holder = format!("{name} (PID {pid})");
        if pid == own_pid {
            holders.this = Some(holder);
        } else {
            holders.others.push(holder);
        }
    }
    holders
}

/// Processes (other than `own_pid`) that have `device` open, as
/// `"<command> (PID <pid>)"` ([`holders_by_pid`]'s others).
pub fn holders_in(proc_root: &Path, device: &Path, own_pid: u32) -> Vec<String> {
    holders_by_pid(proc_root, device, own_pid).others
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
    link == target || fs::canonicalize(link).is_ok_and(|p| p == target)
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

/// The other holders of `device` on this machine, this process left out
/// (empty where `/proc` does not exist).
pub fn holders(device: &str) -> Vec<String> {
    on_this_machine(device).others
}

/// This process as a holder of `device`, `"<command> (PID <pid>)"` like the
/// others, when it has it open itself (`None` where `/proc` does not exist).
pub fn held_here(device: &str) -> Option<String> {
    on_this_machine(device).this
}

fn on_this_machine(device: &str) -> Holders {
    holders_by_pid(Path::new("/proc"), Path::new(device), std::process::id())
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

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
    fn this_process_is_told_apart_from_other_holders() {
        // D-2026-10-01-live-screen-controls-4: PID 42 is this process.
        let root = fake_proc("this");
        let device = root.join("ttyACM1");
        fs::write(&device, b"").unwrap();
        let other = root.join("ttyACM0");
        fs::write(&other, b"").unwrap();
        process(
            &root,
            42,
            &["/usr/bin/bezel-studio"],
            "bezel-studio",
            &[&device],
        );
        process(&root, 99, &[], "TURZX", &[&device]);
        process(
            &root,
            2472,
            &["/venv/bin/python", "main.py"],
            "python",
            &[&other],
        );
        assert_eq!(
            holders_by_pid(&root, &device, 42),
            Holders {
                this: Some("bezel-studio (PID 42)".into()),
                others: vec!["TURZX (PID 99)".into()],
            }
        );
        assert_eq!(holders_in(&root, &device, 42), ["TURZX (PID 99)"]);
        let elsewhere = holders_by_pid(&root, &other, 42);
        assert_eq!(elsewhere.this, None, "this process does not hold it");
        assert_eq!(elsewhere.others, ["python main.py (PID 2472)"]);
        assert_eq!(
            holders_by_pid(Path::new("/no/such/proc"), &device, 42),
            Holders::default()
        );
        fs::remove_dir_all(&root).unwrap();

        // The real `/proc`: a file this process keeps open names it, and only
        // as this process.
        let path = std::env::temp_dir().join(format!("bezel-held-{}", std::process::id()));
        let file = fs::File::create(&path).unwrap();
        let address = path.to_str().unwrap();
        let me = format!("(PID {})", std::process::id());
        assert!(held_here(address).is_some_and(|h| h.ends_with(&me)), "{me}");
        assert!(holders(address).is_empty());
        drop(file);
        assert_eq!(held_here(address), None);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn missing_proc_or_device_yields_nothing() {
        assert!(holders_in(Path::new("/no/such/proc"), Path::new("/dev/null"), 1).is_empty());
        let root = fake_proc("empty");
        assert!(holders_in(&root, &root.join("absent"), 1).is_empty());
        assert!(holders("/dev/bezel-no-such-port").is_empty());
        assert_eq!(held_here("/dev/bezel-no-such-port"), None);
        fs::remove_dir_all(&root).unwrap();
    }
}
