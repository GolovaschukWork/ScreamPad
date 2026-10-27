//! The single place where IPC commands are registered. The TypeScript bindings are generated from this registry.

use tauri_specta::{Builder, collect_commands, collect_events};

use crate::commands;

/// Generic over the runtime so tests can use Tauri's mock runtime; production uses `tauri::Wry`.
pub fn builder<R: tauri::Runtime>() -> Builder<R> {
    Builder::<R>::new()
        .commands(collect_commands![
            commands::app::app_info,
            commands::devices::list_audio_devices,
            commands::library::library_stats,
            commands::library::list_sounds,
            commands::library::check_sound_media,
            commands::library::import_sound,
            commands::playback::play_sound,
            commands::playback::stop_all_sounds,
            commands::playback::stop_sound,
            commands::playback::playing_sounds,
            commands::playback::audio_status,
            commands::playback::retry_audio,
            commands::routing::call_routing_status,
            commands::routing::start_call_routing,
            commands::routing::stop_call_routing,
            commands::routing::set_local_monitor_output,
            commands::routing::open_audio_installer,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::set_sound_options
        ])
        .events(collect_events![commands::playback::PlayingChanged])
}
