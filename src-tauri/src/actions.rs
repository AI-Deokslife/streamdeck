//! 버튼 동작 실행.
use crate::{config::Action, winutil};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::{os::windows::process::CommandExt, process::Command, thread, time::Duration};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// 멀티 액션 한 단계의 최대 대기 시간 (실수로 너무 긴 값 입력 방지)
const MAX_DELAY_MS: u64 = 60_000;

/// 동작 실행.
/// - `from_hotkey`: true면 사용자의 수정키가 떼어질 때까지 기다린다.
/// - `inherited_apps`: 상위 폴더에 연결된 프로그램들. 키/텍스트 입력에 대상이 따로 없으면
///   이 중 열려 있는 프로그램(여러 개면 최근에 쓴 것)으로 보낸다.
pub fn run(app: &AppHandle, action: &Action, from_hotkey: bool, inherited_apps: &[String]) -> Result<(), String> {
    match action {
        Action::Delay { ms } => {
            thread::sleep(Duration::from_millis((*ms).min(MAX_DELAY_MS)));
            Ok(())
        }
        // 폴더 이동은 화면(UI)이 처리한다
        Action::Folder { .. } => Ok(()),
        Action::Multi { steps } => {
            // 수정키 해제 대기는 첫 단계에서 한 번이면 충분
            let mut wait_mods = from_hotkey;
            for (i, step) in steps.iter().enumerate() {
                if matches!(step, Action::Multi { .. } | Action::Folder { .. }) {
                    return Err(format!("{}번째 단계: 멀티 액션/폴더는 단계로 넣을 수 없습니다", i + 1));
                }
                run(app, step, wait_mods, inherited_apps).map_err(|e| format!("{}번째 단계 실패: {e}", i + 1))?;
                if matches!(step, Action::Keys { .. } | Action::Text { .. }) {
                    wait_mods = false;
                }
            }
            Ok(())
        }
        Action::Launch { target, args } => launch(app, target, args),
        Action::Url { url } => app
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| format!("URL 열기 실패: {e}")),
        Action::Script { command } => Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", command])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("스크립트 실행 실패: {e}")),
        Action::Keys { keys, app: target } => {
            prepare_input(from_hotkey, &pick_targets(target, inherited_apps))?;
            send_keys(keys)
        }
        Action::Text { text, app: target, typing } => {
            prepare_input(from_hotkey, &pick_targets(target, inherited_apps))?;
            if *typing {
                let mut enigo = new_enigo()?;
                enigo.text(text).map_err(|e| format!("텍스트 입력 실패: {e}"))
            } else {
                paste_text(text)
            }
        }
    }
}

/// 붙여넣기 전 클립보드에 있던 것 (끝나면 되돌려 놓는다)
enum SavedClipboard {
    Text(String),
    Image(arboard::ImageData<'static>),
    Nothing,
}

/// 붙여넣은 프로그램이 클립보드를 다 읽을 때까지 기다리는 시간 (너무 짧으면 예전 내용이 붙여넣어짐)
const PASTE_SETTLE_MS: u64 = 400;

/// 텍스트를 클립보드에 넣고 Ctrl+V. 원래 클립보드 내용(글자/이미지)은 끝나고 되돌려 놓는다.
/// 한 글자씩 키를 보내는 방식과 달리 한글 입력기 상태와 상관없이 그대로 들어간다.
fn paste_text(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("클립보드를 열지 못했어요: {e}"))?;
    let saved = save_clipboard(&mut cb);
    cb.set_text(text.to_string()).map_err(|e| format!("클립보드에 넣지 못했어요: {e}"))?;
    thread::sleep(Duration::from_millis(30));
    let result = send_keys("ctrl+v");
    thread::sleep(Duration::from_millis(PASTE_SETTLE_MS));
    // 되돌리기 실패는 붙여넣기 자체의 실패가 아니므로 기록만 한다
    if let Err(e) = restore_clipboard(&mut cb, saved) {
        eprintln!("클립보드 되돌리기 실패: {e}");
    }
    result
}

fn save_clipboard(cb: &mut arboard::Clipboard) -> SavedClipboard {
    match cb.get_text() {
        Ok(t) => SavedClipboard::Text(t),
        Err(_) => match cb.get_image() {
            Ok(img) => SavedClipboard::Image(img.to_owned_img()),
            Err(_) => SavedClipboard::Nothing,
        },
    }
}

fn restore_clipboard(cb: &mut arboard::Clipboard, saved: SavedClipboard) -> Result<(), arboard::Error> {
    match saved {
        SavedClipboard::Text(t) => cb.set_text(t),
        SavedClipboard::Image(img) => cb.set_image(img),
        SavedClipboard::Nothing => cb.clear(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_guard_blocks_quick_relaunch() {
        let p = r"C:\Test\Fake_Guard_App.exe";
        assert!(!recently_launched(p));
        mark_launched(p);
        assert!(recently_launched(&p.to_uppercase()), "대소문자 달라도 같은 프로그램");
        assert!(!recently_launched(r"C:\Test\Other.exe"));
    }

    /// 붙여넣기 뒤 원래 클립보드(글자)가 그대로 돌아오는지. 실행 전 클립보드도 마지막에 되돌린다.
    #[test]
    fn clipboard_is_restored_after_paste() {
        let Ok(mut cb) = arboard::Clipboard::new() else { return };
        let user_original = save_clipboard(&mut cb);

        cb.set_text("원래 복사해 둔 글".to_string()).unwrap();
        let saved = save_clipboard(&mut cb);
        cb.set_text("버튼이 넣은 텍스트".to_string()).unwrap();
        restore_clipboard(&mut cb, saved).unwrap();
        assert_eq!(cb.get_text().unwrap(), "원래 복사해 둔 글");

        let _ = restore_clipboard(&mut cb, user_original);
    }
}

/// 버튼에 직접 지정한 프로그램이 있으면 그것, 없으면 폴더에서 물려받은 프로그램들
fn pick_targets(own: &str, inherited: &[String]) -> Vec<String> {
    let own = own.trim();
    if own.is_empty() {
        inherited.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).map(str::to_string).collect()
    } else {
        vec![own.to_string()]
    }
}

/// 키 입력 전 준비: 수정키 해제 대기 → 대상 프로그램 창(또는 직전 창)을 앞으로
fn prepare_input(from_hotkey: bool, targets: &[String]) -> Result<(), String> {
    if from_hotkey {
        winutil::wait_modifiers_released();
    }
    if targets.is_empty() {
        winutil::restore_foreign_focus();
        Ok(())
    } else {
        focus(targets)
    }
}

fn focus(exes: &[String]) -> Result<(), String> {
    let names = exes.join(", ");
    winutil::focus_app(exes).map_err(|e| match (e, exes.len()) {
        (winutil::FocusError::NotRunning, 1) => format!("'{names}' 프로그램이 실행 중이 아닙니다"),
        (winutil::FocusError::NotRunning, _) => format!("연결된 프로그램({names}) 중 실행 중인 것이 없습니다"),
        (winutil::FocusError::Failed, _) => format!("'{names}' 창을 앞으로 가져오지 못했습니다"),
    })
}

/// 폴더를 눌렀을 때 일어난 일
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Activated {
    /// 이미 실행 중이라 창을 앞으로 가져옴
    Focused,
    /// 새로 실행함
    Launched,
    /// 조금 전에 실행해서 아직 뜨는 중 (다시 실행하지 않음)
    Starting,
}

/// 프로그램을 실행한 뒤 창이 뜰 때까지 다시 실행하지 않고 기다려 주는 시간
const LAUNCH_GUARD: Duration = Duration::from_secs(15);
static RECENT_LAUNCHES: std::sync::Mutex<Vec<(String, std::time::Instant)>> = std::sync::Mutex::new(Vec::new());

fn recently_launched(path: &str) -> bool {
    let key = path.to_lowercase();
    let mut list = RECENT_LAUNCHES.lock().unwrap();
    list.retain(|(_, t)| t.elapsed() < LAUNCH_GUARD);
    list.iter().any(|(p, _)| *p == key)
}

fn mark_launched(path: &str) {
    RECENT_LAUNCHES.lock().unwrap().push((path.to_lowercase(), std::time::Instant::now()));
}

/// 프로그램 연결 폴더를 눌렀을 때: 실행 중이면 창을 앞으로, 아니면 실행.
/// 실행 경로가 비어 있으면 설치 위치를 자동으로 찾는다.
/// 연결된 프로그램이 여러 개면(예: 크롬·엣지·웨일) 그중 열려 있는 것을 앞으로 가져오고,
/// 하나도 안 열려 있을 때만 실행한다.
pub fn activate_or_launch(app: &AppHandle, apps: &[String], launch_path: &str) -> Result<Activated, String> {
    let apps: Vec<String> = apps.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).map(str::to_string).collect();
    if !apps.is_empty() {
        match winutil::focus_app(&apps) {
            Ok(()) => return Ok(Activated::Focused),
            Err(winutil::FocusError::Failed) => return focus(&apps).map(|_| Activated::Focused),
            Err(winutil::FocusError::NotRunning) => {}
        }
    }
    let path = match launch_path.trim() {
        // 경로가 비면 연결된 프로그램 순서대로 설치 위치를 찾아 처음 찾은 것을 실행
        "" => apps.iter().find_map(|exe| crate::apppath::resolve(exe)).ok_or_else(|| {
            format!(
                "'{}' 실행 파일을 찾지 못했어요. 편집 → 폴더 선택 → '폴더를 누르면 실행할 프로그램'에 경로를 넣어 주세요",
                apps.join(", ")
            )
        })?,
        p => p.to_string(),
    };
    // 방금 실행해서 아직 창이 안 뜬 경우(한글은 몇 초 걸림) 또 실행하지 않는다
    if recently_launched(&path) {
        return Ok(Activated::Starting);
    }
    launch(app, &path, "")?;
    mark_launched(&path);
    Ok(Activated::Launched)
}

/// 관리자 권한이 필요한 프로그램을 일반 실행하면 나는 오류 (ERROR_ELEVATION_REQUIRED)
const ERROR_ELEVATION_REQUIRED: i32 = 740;

fn launch(app: &AppHandle, target: &str, args: &str) -> Result<(), String> {
    let target = target.trim().trim_matches('"');
    if target.is_empty() {
        return Err("실행할 대상이 비어 있습니다".into());
    }
    // 이름만 적은 경우(StarCraft.exe) 아이콘을 가져올 때처럼 설치 위치를 찾아서 실행한다
    let resolved: Option<String> = if std::path::Path::new(target).exists() {
        Some(target.to_string())
    } else if looks_like_program_name(target) {
        crate::apppath::resolve(target)
    } else {
        None
    };

    match resolved {
        Some(path) if path.to_lowercase().ends_with(".exe") => run_exe(app, &path, args),
        // 바로가기·문서·폴더는 Windows 연결 프로그램으로 (바로가기의 작업 폴더 설정도 그대로 적용됨)
        Some(path) if args.trim().is_empty() => {
            app.opener().open_path(path, None::<&str>).map_err(|e| format!("열기 실패: {e}"))
        }
        Some(path) => Command::new(&path).raw_arg(args).spawn().map(|_| ()).map_err(|e| format!("실행 실패: {e}")),
        // ms-settings:, battlenet:// 같은 주소나 PATH 에 있는 명령은 Windows 에 맡긴다
        None if args.trim().is_empty() => Command::new("cmd")
            .args(["/C", "start", "", target])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("실행 실패: {e}")),
        None => Command::new(target)
            .raw_arg(args)
            .spawn()
            .map(|_| ())
            .map_err(|_| format!("'{target}' 을(를) 찾지 못했어요. 전체 경로를 넣어 주세요")),
    }
}

/// "StarCraft.exe" 처럼 경로 없이 실행 파일 이름만 있는지 (주소나 명령어는 제외)
fn looks_like_program_name(t: &str) -> bool {
    !t.contains(['\\', '/', ':']) && t.to_lowercase().ends_with(".exe")
}

/// exe 를 그 프로그램 폴더에서 실행한다 (게임 등은 자기 폴더에서 실행해야 데이터 파일을 찾는다).
/// 관리자 권한이 필요한 프로그램이면 Windows 권한 확인 창을 거쳐 실행한다.
fn run_exe(app: &AppHandle, path: &str, args: &str) -> Result<(), String> {
    let mut cmd = Command::new(path);
    if let Some(dir) = std::path::Path::new(path).parent() {
        cmd.current_dir(dir);
    }
    if !args.trim().is_empty() {
        cmd.raw_arg(args);
    }
    match cmd.spawn() {
        Ok(_) => Ok(()),
        Err(e) if e.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED) && args.trim().is_empty() => {
            app.opener().open_path(path, None::<&str>).map_err(|e| format!("실행 실패: {e}"))
        }
        Err(e) => Err(format!("실행 실패: {e}")),
    }
}

fn new_enigo() -> Result<Enigo, String> {
    Enigo::new(&Settings::default()).map_err(|e| format!("입력 장치 초기화 실패: {e}"))
}

/// "ctrl+shift+esc" 형식 문자열을 키 입력으로 보낸다.
/// 공백으로 구분하면 여러 조합을 순서대로 보낸다 (예: "ctrl+a ctrl+c").
pub fn send_keys(spec: &str) -> Result<(), String> {
    let mut enigo = new_enigo()?;
    for combo in spec.split_whitespace() {
        let keys = combo
            .split('+')
            .filter(|s| !s.is_empty())
            .map(parse_key)
            .collect::<Result<Vec<_>, _>>()?;
        if keys.is_empty() {
            continue;
        }
        let (last, mods) = keys.split_last().unwrap();
        let err = |e| format!("키 입력 실패: {e}");
        for k in mods {
            enigo.key(*k, Direction::Press).map_err(err)?;
        }
        enigo.key(*last, Direction::Click).map_err(err)?;
        for k in mods.iter().rev() {
            enigo.key(*k, Direction::Release).map_err(err)?;
        }
        thread::sleep(Duration::from_millis(30));
    }
    Ok(())
}

/// 키 이름 → enigo Key. 친숙한 이름과 브라우저 KeyboardEvent.code 둘 다 허용.
fn parse_key(name: &str) -> Result<Key, String> {
    let lower = name.trim().to_lowercase();
    let n = lower
        .strip_prefix("key")
        .filter(|s| s.len() == 1)
        .or_else(|| lower.strip_prefix("digit"))
        .unwrap_or(&lower);

    let key = match n {
        "ctrl" | "control" | "controlleft" | "controlright" => Key::Control,
        "alt" | "altleft" | "altright" => Key::Alt,
        "shift" | "shiftleft" | "shiftright" => Key::Shift,
        "win" | "super" | "meta" | "metaleft" | "metaright" | "cmd" => Key::Meta,
        "enter" | "return" => Key::Return,
        "esc" | "escape" => Key::Escape,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "insert" | "ins" => Key::Insert,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "pgup" => Key::PageUp,
        "pagedown" | "pgdn" => Key::PageDown,
        "up" | "arrowup" => Key::UpArrow,
        "down" | "arrowdown" => Key::DownArrow,
        "left" | "arrowleft" => Key::LeftArrow,
        "right" | "arrowright" => Key::RightArrow,
        "capslock" => Key::CapsLock,
        "printscreen" | "prtsc" => Key::PrintScr,
        "volumeup" => Key::VolumeUp,
        "volumedown" => Key::VolumeDown,
        "mute" | "volumemute" => Key::VolumeMute,
        "playpause" | "mediaplaypause" => Key::MediaPlayPause,
        "next" | "mediatracknext" => Key::MediaNextTrack,
        "prev" | "mediatrackprevious" => Key::MediaPrevTrack,
        "minus" => Key::Unicode('-'),
        "equal" => Key::Unicode('='),
        "comma" => Key::Unicode(','),
        "period" => Key::Unicode('.'),
        "slash" => Key::Unicode('/'),
        "semicolon" => Key::Unicode(';'),
        "quote" => Key::Unicode('\''),
        "backquote" => Key::Unicode('`'),
        "bracketleft" => Key::Unicode('['),
        "bracketright" => Key::Unicode(']'),
        "backslash" => Key::Unicode('\\'),
        _ => {
            // F1~F24
            if let Some(num) = n.strip_prefix('f').and_then(|s| s.parse::<u32>().ok()) {
                if (1..=24).contains(&num) {
                    return Ok(Key::Other(0x70 + num - 1)); // VK_F1 = 0x70
                }
            }
            // Numpad0~9
            if let Some(num) = n.strip_prefix("numpad").and_then(|s| s.parse::<u32>().ok()) {
                if num <= 9 {
                    return Ok(Key::Other(0x60 + num)); // VK_NUMPAD0 = 0x60
                }
            }
            let mut chars = n.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Unicode(c),
                _ => return Err(format!("알 수 없는 키 이름: {name}")),
            }
        }
    };
    Ok(key)
}

#[cfg(test)]
mod launch_tests {
    use super::looks_like_program_name;

    #[test]
    fn program_name_detection() {
        assert!(looks_like_program_name("StarCraft.exe"));
        assert!(looks_like_program_name("Hwp.EXE"));
        assert!(!looks_like_program_name(r"C:\Games\StarCraft.exe"), "경로는 그대로 사용");
        assert!(!looks_like_program_name("ms-settings:display"), "주소는 Windows 에 맡김");
        assert!(!looks_like_program_name("battlenet://SC1"));
        assert!(!looks_like_program_name("notepad"), "확장자 없는 명령은 PATH 로");
    }
}
