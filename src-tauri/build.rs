fn main() {
    // On Windows the webview runtime needs the Common Controls v6 side-by-side manifest, otherwise the executable fails to
    // start with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139). `tauri-build` embeds it in the application binary only, so the
    // integration tests that drive the Tauri mock runtime (`tests/ipc.rs`) get it here. `rustc-link-arg-tests` reaches
    // integration tests but not the library's unit tests, which therefore must not need the webview runtime.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
    tauri_build::build()
}
