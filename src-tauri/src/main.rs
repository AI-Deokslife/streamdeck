// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Claude Code 상태 표시줄에서 불릴 때: 사용량만 적어 두고 바로 끝낸다 (창을 띄우지 않음)
    if std::env::args().any(|a| a == "--statusline") {
        std::process::exit(tauri_app_lib::statusline_main());
    }
    tauri_app_lib::run()
}
