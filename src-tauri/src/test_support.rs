//! Helpers shared by unit tests.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A unique temporary directory removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("screampad-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Managed files are read-only, which Windows refuses to delete: lift the flag everywhere first.
        make_writable(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[allow(clippy::permissions_set_readonly_false)]
fn make_writable(directory: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            make_writable(&path);
        } else if let Ok(metadata) = std::fs::metadata(&path) {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(false);
            let _ = std::fs::set_permissions(&path, permissions);
        }
    }
}
