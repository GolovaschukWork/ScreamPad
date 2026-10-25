//! Use cases and the ports they need. Nothing here depends on Tauri, SQLite, the file system or audio libraries.

pub mod active;
pub mod cancel;
pub mod clip_cache;
pub mod error;
pub mod import;
pub mod playback;
pub mod ports;
pub mod reconcile;

#[cfg(test)]
mod import_tests;
#[cfg(test)]
mod playback_tests;
#[cfg(test)]
mod reconcile_tests;
#[cfg(test)]
mod test_env;
