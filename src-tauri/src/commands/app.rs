use serde::Serialize;
use specta::Type;

/// Static information about the running application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

#[tauri::command]
#[specta::specta]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "ScreamPad".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}
