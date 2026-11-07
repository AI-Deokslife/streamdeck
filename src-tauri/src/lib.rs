mod actions;
mod apppath;
mod config;
mod winutil;

use config::{Action, Config};
use std::{collections::HashMap, sync::Mutex, thread};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, LogicalSize, Manager, State, WebviewWindow,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const TOGGLE_ID: &str = "__toggle__";
/// 시작 시 창 토글 단축키가 다른 프로그램과 겹치면 차례로 시도할 대체 키
const FALLBACK_TOGGLES: &[&str] = &["Ctrl+Shift+F12", "Ctrl+Shift+F11", "Ctrl+Alt+Shift+D", "Ctrl+Shift+F10"];

#[derive(Default)]
struct AppState {
    config: Mutex<Config>,
    /// 등록된 전역 단축키 id → 버튼 id (또는 TOGGLE_ID)
    hotkeys: Mutex<HashMap<u32, String>>,
    /// 편집 모드 진입 전 창 크기
    saved_size: Mutex<Option<LogicalSize<f64>>>,
    /// 앱 시작 시 단축키 등록 과정에서 생긴 알림 (화면에 한 번 보여줌)
    startup_notices: Mutex<Vec<String>>,
}

fn toggle_registered(app: &AppHandle) -> bool {
    app.state::<AppState>().hotkeys.lock().unwrap().values().any(|v| v == TOGGLE_ID)
}

/// 시작 시 단축키 등록. 창 토글 키가 실패하면 대체 키로 바꾸고 설정에 저장한다.
fn register_on_startup(app: &AppHandle, cfg: &mut Config) -> Vec<String> {
    let mut notices = register_hotkeys(app, cfg);
    if cfg.toggle_hotkey.trim().is_empty() || toggle_registered(app) {
        return notices;
    }
    let old = cfg.toggle_hotkey.clone();
    for cand in FALLBACK_TOGGLES.iter().filter(|c| **c != old) {
        let mut trial = cfg.clone();
        trial.toggle_hotkey = cand.to_string();
        let trial_notices = register_hotkeys(app, &trial);
        if toggle_registered(app) {
            *cfg = trial;
            let _ = config::save(app, cfg);
            notices = trial_notices;
            notices.push(format!(
                "창 보이기/숨기기 단축키 '{old}'를 다른 프로그램이 쓰고 있어서 '{cand}'로 바꿨어요. 편집(✏️)에서 변경할 수 있어요."
            ));
            return notices;
        }
    }
    notices
}

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

fn toggle_window(app: &AppHandle) {
    if let Some(w) = main_window(app) {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
        }
    }
}

fn show_window(app: &AppHandle) {
    if let Some(w) = main_window(app) {
        let _ = w.show();
        let _ = w.unminimize();
    }
}

/// 설정에 있는 단축키를 모두 다시 등록한다. 실패한 항목은 경고 문자열로 돌려준다.
fn register_hotkeys(app: &AppHandle, cfg: &Config) -> Vec<String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let state = app.state::<AppState>();
    let mut map = state.hotkeys.lock().unwrap();
    map.clear();
    let mut warnings = Vec::new();

    let entries = std::iter::once((TOGGLE_ID.to_string(), "창 보이기/숨기기".to_string(), cfg.toggle_hotkey.clone()))
        .chain(
            config::all_buttons(&cfg.buttons)
                .into_iter()
                .map(|b| (b.id.clone(), b.label.clone(), b.hotkey.clone())),
        );

    for (id, label, hotkey) in entries {
        if hotkey.trim().is_empty() {
            continue;
        }
        let shortcut: Shortcut = match hotkey.parse() {
            Ok(s) => s,
            Err(e) => {
                warnings.push(format!("[{label}] 단축키 형식 오류 '{hotkey}': {e}"));
                continue;
            }
        };
        if map.contains_key(&shortcut.id()) {
            warnings.push(format!("[{label}] 단축키 '{hotkey}'가 다른 버튼과 겹칩니다"));
            continue;
        }
        match gs.register(shortcut) {
            Ok(_) => {
                map.insert(shortcut.id(), id);
            }
            Err(e) => warnings.push(format!("[{label}] '{hotkey}' 등록 실패 (다른 프로그램이 사용 중일 수 있음): {e}")),
        }
    }
    warnings
}

/// 버튼 동작과, 상위 폴더에서 물려받은 대상 프로그램들
fn find_action(app: &AppHandle, button_id: &str) -> Option<(Action, Vec<String>)> {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap();
    config::find_button(&cfg.buttons, button_id).map(|(b, inherited)| (b.action.clone(), inherited))
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppFocusEvent {
    /// 앞에 온 프로그램 (예: Hwp.exe)
    exe: String,
    /// 그 프로그램에 연결된 폴더 (없으면 null)
    folder_id: Option<String>,
}

// ---------------- 프론트엔드에서 호출하는 명령 ----------------

#[tauri::command]
fn get_config(state: State<AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

/// 시작 알림을 한 번만 돌려준다
#[tauri::command]
fn take_startup_notices(state: State<AppState>) -> Vec<String> {
    std::mem::take(&mut *state.startup_notices.lock().unwrap())
}

#[tauri::command]
fn save_config(app: AppHandle, state: State<AppState>, cfg: Config) -> Result<Vec<String>, String> {
    config::save(&app, &cfg)?;
    *state.config.lock().unwrap() = cfg.clone();
    Ok(register_hotkeys(&app, &cfg))
}

#[tauri::command]
async fn run_button(app: AppHandle, id: String) -> Result<(), String> {
    let (action, inherited) = find_action(&app, &id).ok_or("버튼을 찾을 수 없습니다")?;
    tauri::async_runtime::spawn_blocking(move || actions::run(&app, &action, false, &inherited))
        .await
        .map_err(|e| e.to_string())?
}

/// 편집 중인(아직 저장 안 된) 동작을 바로 실행해 보는 용도
#[tauri::command]
async fn test_action(app: AppHandle, action: Action, inherited_apps: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || actions::run(&app, &action, false, &inherited_apps))
        .await
        .map_err(|e| e.to_string())?
}

/// 지금 앞에 있는 프로그램과 연결된 폴더 (편집 후/시작 시 다시 맞추기용)
#[tauri::command]
fn current_app(state: State<AppState>) -> AppFocusEvent {
    let exe = winutil::last_app();
    let folder_id = config::folder_for_app(&state.config.lock().unwrap().buttons, &exe);
    AppFocusEvent { exe, folder_id }
}

/// 지금 창이 열려 있는 프로그램 목록 (프로그램 연결할 때 고르기용)
#[tauri::command]
fn list_running_apps() -> Vec<winutil::RunningApp> {
    winutil::running_apps()
}

/// 프로그램 연결 폴더를 눌렀을 때: 연결된 프로그램 중 열려 있는 것을 앞으로, 하나도 없으면 실행
#[tauri::command]
async fn activate_app(app: AppHandle, apps: Vec<String>, launch: String) -> Result<actions::Activated, String> {
    tauri::async_runtime::spawn_blocking(move || actions::activate_or_launch(&app, &apps, &launch))
        .await
        .map_err(|e| e.to_string())?
}

/// 실행 파일 이름으로 설치 경로 찾기 (예: Hwp.exe → C:\...\Hwp.exe). 못 찾으면 null
#[tauri::command]
async fn resolve_app_path(exe: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || apppath::resolve(&exe)).await.ok().flatten()
}

#[tauri::command]
fn hide_window(app: AppHandle) {
    if let Some(w) = main_window(&app) {
        let _ = w.hide();
    }
}

/// 편집 모드: 글자 입력이 가능하도록 포커스 허용 + 창 확대. 해제 시 원복.
#[tauri::command]
fn set_edit_mode(app: AppHandle, state: State<AppState>, on: bool) -> Result<(), String> {
    let w = main_window(&app).ok_or("창 없음")?;
    let scale = w.scale_factor().map_err(|e| e.to_string())?;
    if on {
        winutil::set_no_activate(false);
        let size = w.inner_size().map_err(|e| e.to_string())?.to_logical::<f64>(scale);
        *state.saved_size.lock().unwrap() = Some(size);
        let target = LogicalSize::new(size.width.max(460.0), size.height.max(620.0));
        let _ = w.set_size(target);
        let _ = w.set_focus();
        // 단축키를 녹화할 때 이미 등록된 전역 단축키가 먼저 가로채지 않도록 편집 중에는 잠시 해제
        let _ = app.global_shortcut().unregister_all();
        state.hotkeys.lock().unwrap().clear();
    } else {
        if let Some(size) = state.saved_size.lock().unwrap().take() {
            let _ = w.set_size(size);
        }
        winutil::set_no_activate(true);
        let cfg = state.config.lock().unwrap().clone();
        // 경고는 저장할 때(save_config) 이미 화면에 보여줬으므로 여기서는 기록만
        for warn in register_hotkeys(&app, &cfg) {
            eprintln!("{warn}");
        }
    }
    Ok(())
}

// ---------------- 트레이 ----------------

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let toggle = MenuItem::with_id(app, "toggle", "보이기 / 숨기기", true, None::<&str>)?;
    let edit = MenuItem::with_id(app, "edit", "버튼 편집", true, None::<&str>)?;
    let auto = CheckMenuItem::with_id(app, "autostart", "Windows 시작 시 자동 실행", true, autostart_on, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&toggle, &edit, &sep, &auto, &quit])?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("앱 아이콘 없음"))
        .tooltip("QuickDeck")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => toggle_window(app),
            "edit" => {
                show_window(app);
                let _ = app.emit("open-editor", ());
            }
            "autostart" => {
                let al = app.autolaunch();
                let _ = if al.is_enabled().unwrap_or(false) { al.disable() } else { al.enable() };
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

// ---------------- 진입점 ----------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 이미 실행 중이면 새로 띄우지 않고 기존 창을 보여준다 (반드시 첫 번째 플러그인)
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all() - tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let id = app.state::<AppState>().hotkeys.lock().unwrap().get(&shortcut.id()).cloned();
                    match id.as_deref() {
                        Some(TOGGLE_ID) => toggle_window(app),
                        Some(button_id) => {
                            if let Some((action, inherited)) = find_action(app, button_id) {
                                if let Action::Folder { apps, launch, .. } = &action {
                                    // 폴더 단축키: 창을 띄우고 그 폴더를 연다 (+ 연결 프로그램 실행/활성화)
                                    show_window(app);
                                    let _ = app.emit("open-folder", button_id.to_string());
                                    if !launch.trim().is_empty() || !apps.is_empty() {
                                        let (app, apps, launch) = (app.clone(), apps.clone(), launch.clone());
                                        thread::spawn(move || {
                                            if let Err(e) = actions::activate_or_launch(&app, &apps, &launch) {
                                                let _ = app.emit("action-error", e);
                                            }
                                        });
                                    }
                                    return;
                                }
                                let app = app.clone();
                                thread::spawn(move || {
                                    if let Err(e) = actions::run(&app, &action, true, &inherited) {
                                        let _ = app.emit("action-error", e);
                                    }
                                });
                            }
                        }
                        None => {}
                    }
                })
                .build(),
        )
        .manage(AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let mut cfg = config::load(&handle);

            if let Some(w) = main_window(&handle) {
                if let Ok(hwnd) = w.hwnd() {
                    winutil::set_own_hwnd(hwnd.0 as isize);
                }
                winutil::set_no_activate(true);
            }
            // 활성 프로그램이 바뀌면 화면에 알린다 (연결된 폴더 자동 열기 = 스마트 프로필)
            let tracker_app = handle.clone();
            winutil::start_foreground_tracker(move |exe| {
                let folder_id = {
                    let state = tracker_app.state::<AppState>();
                    let cfg = state.config.lock().unwrap();
                    config::folder_for_app(&cfg.buttons, &exe)
                };
                let _ = tracker_app.emit("app-focus", AppFocusEvent { exe, folder_id });
            });

            let notices = register_on_startup(&handle, &mut cfg);
            for n in &notices {
                eprintln!("{n}");
            }
            let state = app.state::<AppState>();
            *state.config.lock().unwrap() = cfg;
            *state.startup_notices.lock().unwrap() = notices;
            build_tray(&handle)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            take_startup_notices,
            save_config,
            run_button,
            test_action,
            list_running_apps,
            current_app,
            activate_app,
            resolve_app_path,
            hide_window,
            set_edit_mode
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
