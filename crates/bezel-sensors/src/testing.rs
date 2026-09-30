//! Test support: throw-away `/sys` and `/proc` trees under the system temp
//! directory, removed when dropped.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A directory tree that disappears with the value.
pub(crate) struct FakeTree {
    root: PathBuf,
}

impl FakeTree {
    /// An empty tree with a unique name.
    pub(crate) fn new(name: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("bezel-sensors-{name}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    /// Absolute path of `rel`.
    pub(crate) fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// Root of the tree.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Writes `contents` to `rel`, creating parent directories.
    pub(crate) fn file(&self, rel: &str, contents: &str) -> &Self {
        let path = self.path(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
        self
    }

    /// Creates the directory `rel`.
    pub(crate) fn dir(&self, rel: &str) -> &Self {
        fs::create_dir_all(self.path(rel)).unwrap();
        self
    }

    /// Makes `rel` a symbolic link to `target` (relative to the tree root).
    pub(crate) fn link(&self, rel: &str, target: &str) -> &Self {
        let path = self.path(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        std::os::unix::fs::symlink(self.path(target), path).unwrap();
        self
    }
}

impl Drop for FakeTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
