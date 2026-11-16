// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // v0.9.11 의 Claude 상태 표시줄 연결이 남아 있어도 창을 띄우지 않고 조용히 끝낸다
    if std::env::args().any(|a| a == "--statusline") {
        return;
    }
    tauri_app_lib::run()
}
