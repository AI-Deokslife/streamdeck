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
        Action::Text { text, app: target } => {
            prepare_input(from_hotkey, &pick_targets(target, inherited_apps))?;
            let mut enigo = new_enigo()?;
            enigo.text(text).map_err(|e| format!("텍스트 입력 실패: {e}"))
        }
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
    launch(app, &path, "").map(|_| Activated::Launched)
}

fn launch(app: &AppHandle, target: &str, args: &str) -> Result<(), String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("실행할 대상이 비어 있습니다".into());
    }
    if args.trim().is_empty() {
        // 경로/파일/폴더는 연결 프로그램으로, 이름만 있으면(notepad.exe) 그대로 실행
        if std::path::Path::new(target).exists() {
            return app
                .opener()
                .open_path(target, None::<&str>)
                .map_err(|e| format!("열기 실패: {e}"));
        }
        return Command::new("cmd")
            .args(["/C", "start", "", target])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("실행 실패: {e}"));
    }
    Command::new(target)
        .raw_arg(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("실행 실패: {e}"))
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
