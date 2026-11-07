//! 버튼 동작 실행.
use crate::{config::Action, winutil};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::{os::windows::process::CommandExt, process::Command, thread, time::Duration};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 동작 실행. `from_hotkey`가 true면 사용자의 수정키가 떼어질 때까지 기다린다.
pub fn run(app: &AppHandle, action: &Action, from_hotkey: bool) -> Result<(), String> {
    match action {
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
        Action::Keys { keys } => {
            prepare_input(from_hotkey);
            send_keys(keys)
        }
        Action::Text { text } => {
            prepare_input(from_hotkey);
            let mut enigo = new_enigo()?;
            enigo.text(text).map_err(|e| format!("텍스트 입력 실패: {e}"))
        }
    }
}

fn prepare_input(from_hotkey: bool) {
    if from_hotkey {
        winutil::wait_modifiers_released();
    }
    winutil::restore_foreign_focus();
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
