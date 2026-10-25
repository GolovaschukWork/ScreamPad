//! Tauri command adapters. They validate input and delegate to application services; they hold no business logic.
//! IPC DTOs live here so `domain` and `application` stay independent of the IPC library (ADR 0004).
//! Commands are registered by their full module path in `crate::ipc` (the specta macros need it).

pub mod app;
pub mod error;
pub mod library;
pub mod playback;
