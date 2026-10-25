//! Adapters to the outside world (storage, media). `domain` and `application` never import from here.

pub mod audio;
pub mod dialog;
pub mod media;
pub mod sqlite;
pub mod system;
