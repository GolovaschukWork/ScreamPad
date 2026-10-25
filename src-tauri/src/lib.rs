mod commands;
mod ipc;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(ipc::builder::<tauri::Wry>().invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
