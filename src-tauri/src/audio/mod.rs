//! The real-time audio core (ADR 0006). Nothing in here touches a device, the file system, SQLite or Tauri, so it can be
//! tested without audio hardware. The render path is allocation-free and never blocks.

pub mod clip;
pub mod convert;
pub mod mixer;
