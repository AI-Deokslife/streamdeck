mod actions;
mod apppath;
mod autohide;
mod capture;
mod config;
mod favicon;
mod icon;
mod media;
mod receive;
mod remote;
mod widgets;
mod updates;
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
    /// 토글 버튼 상태 (버튼 id → 켜짐). 앱을 껐다 켜면 모두 꺼짐으로 시작
    toggles: Mutex<HashMap<String, bool>>,
}

/// 버튼 실행 (PC 클릭·전역 단축키·휴대폰 공통). 토글 상태와 길게 누르기를 여기서 처리한다
fn execute(app: &AppHandle, id: &str, long: bool, from_hotkey: bool) -> Result<(), String> {
    let (button, inherited) = {
        let state = app.state::<AppState>();
        let cfg = state.config.lock().unwrap();
        config::find_button(&cfg.buttons, id).map(|(b, i)| (b.clone(), i))
    }
    .ok_or("버튼을 찾을 수 없어요")?;
    let action = if long {
        match (&button.long_action, &button.action) {
            (Some(a), _) => a.clone(),
            (None, Action::Timer { .. }) => {
                widgets::timer_reset(id);
                remote::bump_version(app);
                return Ok(());
            }
            (None, _) => return Err("길게 누르기 동작이 없어요".into()),
        }
    } else {
        button.action.clone()
    };
    match &action {
        // 타이머: 짧게 = 시작/일시정지, 길게(따로 정한 동작이 없으면) = 처음으로
        Action::Timer { minutes } => {
            widgets::timer_press(id, *minutes);
            remote::bump_version(app);
            Ok(())
        }
        Action::Toggle { first, second, .. } => {
            let on = app.state::<AppState>().toggles.lock().unwrap().get(id).copied().unwrap_or(false);
            actions::run(app, if on { second } else { first }, from_hotkey, &inherited)?;
            app.state::<AppState>().toggles.lock().unwrap().insert(id.to_string(), !on);
            let _ = app.emit("toggle-changed", serde_json::json!({ "id": id, "on": !on }));
            remote::bump_version(app);
            Ok(())
        }
        _ => actions::run(app, &action, from_hotkey, &inherited),
    }
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
        // 가장자리에 숨어 있었으면 숨기기 대신 꺼내기
        if autohide::reveal() {
            return;
        }
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
        }
    }
}

pub(crate) fn show_window(app: &AppHandle) {
    autohide::reveal();
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
    autohide::set_enabled(cfg.auto_hide);
    *state.config.lock().unwrap() = cfg.clone();
    remote::bump_version(&app); // 휴대폰 리모컨 화면도 새로 받게
    Ok(register_hotkeys(&app, &cfg))
}

/// 휴대폰 리모컨에서 버튼을 눌렀을 때 (PC 의 버튼 클릭과 같은 동작)
fn press_from_remote(app: &AppHandle, id: &str, long: bool) -> Result<(), String> {
    if long {
        return execute(app, id, true, false);
    }
    let (action, _) = find_action(app, id).ok_or("버튼을 찾을 수 없어요")?;
    match &action {
        // 폴더 이동은 휴대폰 화면이 처리하고, 연결 프로그램이 있으면 PC 에서 실행/활성화
        Action::Folder { apps, launch, .. } if !launch.trim().is_empty() || !apps.is_empty() => {
            actions::activate_or_launch(app, apps, launch).map(|_| ())
        }
        Action::Folder { .. } => Ok(()),
        _ => execute(app, id, false, false),
    }
}

/// 위젯 버튼에 보여줄 값 (CPU·메모리, 타이머 남은 시간)
#[tauri::command]
fn get_widgets() -> widgets::WidgetState {
    widgets::snapshot()
}

/// 토글 버튼들의 지금 상태
#[tauri::command]
fn get_toggles(state: State<AppState>) -> HashMap<String, bool> {
    state.toggles.lock().unwrap().clone()
}

#[tauri::command]
fn remote_status(app: AppHandle) -> remote::RemoteStatus {
    remote::status(&app)
}

#[tauri::command]
fn set_remote(app: AppHandle, enabled: bool) -> remote::RemoteStatus {
    remote::set_enabled(&app, enabled)
}

#[tauri::command]
fn regenerate_remote_key(app: AppHandle) -> remote::RemoteStatus {
    remote::regenerate(&app)
}

/// 휴대폰에서 받은 파일: 폴더·클립보드 설정
#[tauri::command]
fn set_remote_receive(app: AppHandle, clip_image: Option<bool>, reset_dir: Option<bool>) -> remote::RemoteStatus {
    remote::set_receive(&app, reset_dir.filter(|r| *r).map(|_| String::new()), clip_image)
}

/// 받을 폴더 고르기 (취소하면 그대로)
#[tauri::command]
async fn pick_receive_dir(app: AppHandle) -> Result<remote::RemoteStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let start = remote::receive_dir_now(&app).ok();
        let mut dlg = app.dialog().file().set_title("휴대폰에서 받은 파일을 둘 폴더");
        if let Some(d) = start {
            dlg = dlg.set_directory(d);
        }
        if let Some(folder) = dlg.blocking_pick_folder() {
            let path = folder.into_path().map_err(|e| e.to_string())?;
            return Ok(remote::set_receive(&app, Some(path.display().to_string()), None));
        }
        Ok(remote::status(&app))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 한꺼번에 받은 사진: 클립보드를 다음(1)·이전(-1) 사진으로, 0 이면 '모두'로
#[tauri::command]
fn clip_step(app: AppHandle, step: i32) -> Result<serde_json::Value, String> {
    remote::clip_step(&app, step.clamp(-1, 1))
}

/// 받은 파일 폴더 열기
#[tauri::command]
fn open_receive_dir(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = remote::receive_dir_now(&app)?;
    app.opener().open_path(dir.display().to_string(), None::<&str>).map_err(|e| e.to_string())
}

/// USB 테더링 연결에서도 휴대폰이 접속할 수 있게 방화벽 규칙 추가 (관리자 확인 창이 뜬다)
#[tauri::command]
async fn allow_usb_remote(app: AppHandle) -> Result<remote::RemoteStatus, String> {
    remote::allow_usb(&app)
}

#[tauri::command]
async fn run_button(app: AppHandle, id: String, long: Option<bool>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || execute(&app, &id, long.unwrap_or(false), false))
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

/// 프로그램 아이콘을 버튼 이미지용 PNG data URL 로.
/// `target`은 전체 경로(C:\...\Hwp.exe) 또는 실행 파일 이름(Hwp.exe — 설치 위치를 찾아서 사용)
#[tauri::command]
async fn get_app_icon(target: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let t = target.trim().trim_matches('"');
        let path = if std::path::Path::new(t).is_file() {
            t.to_string()
        } else {
            apppath::resolve(t).ok_or_else(|| format!("'{t}' 파일을 찾지 못했어요"))?
        };
        icon::file_icon_png(&path, 128).map(|png| icon::to_data_url(&png))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 웹사이트 아이콘 (URL 버튼 이미지용). 그 사이트에만 직접 접속한다
#[tauri::command]
async fn get_site_icon(url: String) -> Result<favicon::SiteIcon, String> {
    tauri::async_runtime::spawn_blocking(move || favicon::fetch(&url)).await.map_err(|e| e.to_string())?
}

/// 화면 일부를 캡처해서 PNG data URL 로 (Windows 캡처 도구 사용)
#[tauri::command]
async fn capture_screen(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || capture::capture_region(&app).map(|png| icon::to_data_url(&png)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel_capture() {
    capture::cancel();
}

/// 설정을 파일로 내보내기 (편집 중인 내용 그대로). 저장한 경로를 돌려주고, 취소하면 null
#[tauri::command]
async fn export_config(app: AppHandle, cfg: Config) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("QuickDeck 설정", &["json"])
            .set_file_name("QuickDeck 설정.json")
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| format!("내보내기 실패: {e}"))?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 설정 파일 가져오기 (자동 백업 폴더에서 시작). 읽은 설정을 돌려주고, 저장은 화면에서 확인 후 한다
#[tauri::command]
async fn import_config(app: AppHandle) -> Result<Option<Config>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let mut dialog = app.dialog().file().add_filter("QuickDeck 설정", &["json"]);
        if let Ok(dir) = config::backup_dir(&app) {
            dialog = dialog.set_directory(dir);
        }
        let Some(file) = dialog.blocking_pick_file() else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        let text = std::fs::read_to_string(&path).map_err(|e| format!("파일을 읽지 못했어요: {e}"))?;
        config::parse(&text).map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn open_backup_folder(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = config::backup_dir(&app)?;
    app.opener().open_path(dir.display().to_string(), None::<&str>).map_err(|e| e.to_string())
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
    autohide::set_paused(on);
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
    let update = MenuItem::with_id(app, "update", "업데이트 확인", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    // 맨 아래: 지금 버전과 만든 사람 (누를 수 없는 안내 줄)
    let version = app.package_info().version.to_string();
    let about = MenuItem::with_id(app, "about", format!("QuickDeck v{version} · 제작 Lee EunDeok"), false, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &edit, &sep, &auto, &update, &quit, &sep2, &about])?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("앱 아이콘 없음"))
        .tooltip(format!("QuickDeck v{}", app.package_info().version))
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
            "update" => {
                show_window(app);
                updates::check_in_background(app.clone(), true, std::time::Duration::ZERO);
            }
            "quit" => {
                // 숨은 자리 말고 원래 자리를 기억하도록 먼저 꺼낸다
                autohide::reveal();
                app.exit(0)
            }
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
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
                                let _ = inherited;
                                let (app, id) = (app.clone(), button_id.to_string());
                                thread::spawn(move || {
                                    if let Err(e) = execute(&app, &id, false, true) {
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
        .manage(remote::RemoteState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let (mut cfg, load_notice) = config::load(&handle);

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

            let mut notices = register_on_startup(&handle, &mut cfg);
            notices.extend(load_notice);
            for n in &notices {
                eprintln!("{n}");
            }
            let state = app.state::<AppState>();
            autohide::set_enabled(cfg.auto_hide);
            autohide::start();
            // 저장된 위치가 화면 밖이면 안으로 (부팅 직후엔 모니터가 늦게 잡힐 수 있어 몇 번 더 확인)
            thread::spawn(|| {
                for secs in [0u64, 2, 6, 15] {
                    thread::sleep(std::time::Duration::from_secs(secs.max(1)));
                    autohide::ensure_on_screen();
                }
            });
            *state.config.lock().unwrap() = cfg;
            *state.startup_notices.lock().unwrap() = notices;
            build_tray(&handle)?;
            remote::start_if_enabled(&handle);
            widgets::start_watcher(handle.clone());
            // 켠 뒤 잠시 있다가 새 버전 확인 (조용히: 없거나 실패하면 아무 말 안 함)
            updates::start(handle.clone());
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
            get_toggles,
            get_widgets,
            updates::install_update,
            updates::snooze_update,
            activate_app,
            resolve_app_path,
            get_app_icon,
            capture_screen,
            get_site_icon,
            cancel_capture,
            export_config,
            import_config,
            open_backup_folder,
            remote_status,
            set_remote,
            regenerate_remote_key,
            allow_usb_remote,
            set_remote_receive,
            pick_receive_dir,
            open_receive_dir,
            clip_step,
            hide_window,
            set_edit_mode
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
