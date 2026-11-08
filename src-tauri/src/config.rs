//! 설정 파일(config.json) 구조와 로드/저장.
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

/// 버튼이 실행하는 동작.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Action {
    /// 프로그램/파일/폴더 실행
    Launch {
        target: String,
        #[serde(default)]
        args: String,
    },
    /// 단축키 보내기 (예: "ctrl+shift+esc", "volumeup")
    Keys {
        keys: String,
        /// 이 프로그램 창으로 보낸다 (예: "Hwp.exe"). 비면 상위 폴더의 연결 프로그램 → 현재 활성 창
        #[serde(default)]
        app: String,
    },
    /// 웹 주소 열기
    Url { url: String },
    /// 텍스트 입력 (스니펫)
    Text {
        text: String,
        #[serde(default)]
        app: String,
        /// true면 한 글자씩 키 입력으로 보낸다 (붙여넣기를 막아 둔 프로그램용).
        /// 기본(false)은 클립보드에 넣고 Ctrl+V — 한글·긴 글도 깨지지 않는다
        #[serde(default)]
        typing: bool,
    },
    /// PowerShell 명령 실행
    Script { command: String },
    /// 기다리기 (멀티 액션의 단계로 사용)
    Delay { ms: u64 },
    /// 여러 동작을 순서대로 실행
    Multi { steps: Vec<Action> },
    /// 시계 (버튼에 지금 시각 표시, 누르면 아무 일 없음)
    Clock {},
    /// 타이머: 누르면 시작/일시정지, 길게 누르면 처음으로
    Timer { minutes: u32 },
    /// CPU·메모리 사용률 표시
    SysInfo {},
    /// 지금 앞에 있는 창 배치: left / right / maximize / minimize / center / nextMonitor
    Window { mode: String },
    /// 누를 때마다 first / second 를 번갈아 실행 (예: 마이크 끄기 ↔ 켜기).
    /// 켜진 상태(second 를 실행할 차례)에는 alt_label / alt_icon 을 보여준다
    Toggle {
        first: Box<Action>,
        second: Box<Action>,
        #[serde(default)]
        alt_label: String,
        #[serde(default)]
        alt_icon: String,
    },
    /// 하위 폴더: 누르면 안의 버튼들로 화면이 바뀐다 (여러 단계로 중첩 가능)
    Folder {
        #[serde(default)]
        buttons: Vec<Button>,
        /// 연결된 프로그램 (예: ["Hwp.exe"]). 이 프로그램이 앞에 오면 폴더가 자동으로 열린다 (스마트 프로필)
        #[serde(default)]
        apps: Vec<String>,
        /// 폴더를 누를 때 실행할 프로그램 경로. 이미 실행 중이면 그 창을 앞으로 가져온다
        #[serde(default)]
        launch: String,
    },
}

/// id 버튼과, 그 버튼이 속한 가장 가까운 "프로그램 연결 폴더"의 프로그램 목록 전체
/// (예: 웹 브라우저 폴더 → [chrome.exe, msedge.exe, whale.exe] 중 열려 있는 것으로 보낸다)
pub fn find_button<'a>(buttons: &'a [Button], id: &str) -> Option<(&'a Button, Vec<String>)> {
    fn walk<'a>(list: &'a [Button], id: &str, inherited: &'a [String]) -> Option<(&'a Button, Vec<String>)> {
        for b in list {
            if b.id == id {
                return Some((b, inherited.to_vec()));
            }
            if let Action::Folder { buttons, apps, .. } = &b.action {
                let next: &[String] = if apps.is_empty() { inherited } else { apps };
                if let Some(found) = walk(buttons, id, next) {
                    return Some(found);
                }
            }
        }
        None
    }
    walk(buttons, id, &[])
}

/// exe 프로그램에 연결된 폴더 id (가장 먼저 찾은 것)
pub fn folder_for_app(buttons: &[Button], exe: &str) -> Option<String> {
    all_buttons(buttons).into_iter().find_map(|b| match &b.action {
        Action::Folder { apps, .. } if apps.iter().any(|a| a.trim().eq_ignore_ascii_case(exe)) => Some(b.id.clone()),
        _ => None,
    })
}

/// 폴더 안까지 포함한 모든 버튼을 펼쳐서 돌려준다.
pub fn all_buttons(buttons: &[Button]) -> Vec<&Button> {
    let mut out = Vec::new();
    fn walk<'a>(list: &'a [Button], out: &mut Vec<&'a Button>) {
        for b in list {
            out.push(b);
            if let Action::Folder { buttons, .. } = &b.action {
                walk(buttons, out);
            }
        }
    }
    walk(buttons, &mut out);
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Button {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: String,
    /// 버튼 이미지 (data:image/png;base64,...). 있으면 이모지 대신 보여준다
    #[serde(default)]
    pub image: String,
    /// true면 이미지를 버튼 전체에 꽉 채운다 (false면 아이콘 크기로)
    #[serde(default)]
    pub image_full: bool,
    /// true면 프로그램/사이트에서 자동으로 가져온 이미지 (대상이 바뀌면 다시 가져와도 됨).
    /// 사용자가 직접 넣은 이미지는 false 라서 자동으로 덮어쓰지 않는다
    #[serde(default)]
    pub image_auto: bool,
    #[serde(default = "default_color")]
    pub color: String,
    /// 이 버튼을 바로 실행하는 전역 단축키 (선택)
    #[serde(default)]
    pub hotkey: String,
    pub action: Action,
    /// 길게 눌렀을 때 실행할 동작 (선택)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_action: Option<Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// 창 보이기/숨기기 전역 단축키
    pub toggle_hotkey: String,
    pub columns: u32,
    pub buttons: Vec<Button>,
    /// 화면 테마: "auto"(Windows 설정 따라감) / "light" / "dark"
    #[serde(default = "default_theme")]
    pub theme: String,
    /// 창 투명도 (20~100, 100 = 불투명)
    #[serde(default = "default_opacity")]
    pub opacity: u32,
    /// true 면 마우스를 올렸을 때만 선명하게(100%), 벗어나면 opacity 로
    #[serde(default)]
    pub clear_on_hover: bool,
    /// 화면 가장자리에 붙이면 마우스가 떠날 때 숨기기
    #[serde(default)]
    pub auto_hide: bool,
}

fn default_opacity() -> u32 {
    100
}

fn default_theme() -> String {
    "auto".into()
}

fn default_color() -> String {
    "#2d3748".into()
}

fn btn(id: &str, label: &str, icon: &str, color: &str, hotkey: &str, action: Action) -> Button {
    Button {
        id: id.into(),
        label: label.into(),
        icon: icon.into(),
        image: String::new(),
        image_full: false,
        image_auto: false,
        color: color.into(),
        hotkey: hotkey.into(),
        action,
        long_action: None,
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            toggle_hotkey: "Ctrl+Shift+F12".into(),
            columns: 4,
            theme: default_theme(),
            opacity: default_opacity(),
            clear_on_hover: false,
            auto_hide: false,
            buttons: vec![
                btn("b1", "메모장", "📝", "#2b6cb0", "", Action::Launch { target: "notepad.exe".into(), args: String::new() }),
                btn("b2", "계산기", "🧮", "#2f855a", "", Action::Launch { target: "calc.exe".into(), args: String::new() }),
                btn("b3", "작업 관리자", "📊", "#c05621", "", Action::Keys { keys: "ctrl+shift+esc".into(), app: String::new() }),
                btn("b4", "화면 캡처", "✂️", "#6b46c1", "", Action::Keys { keys: "win+shift+s".into(), app: String::new() }),
                btn("b5", "볼륨 -", "🔉", "#4a5568", "Ctrl+Alt+Minus", Action::Keys { keys: "volumedown".into(), app: String::new() }),
                btn("b6", "볼륨 +", "🔊", "#4a5568", "Ctrl+Alt+Equal", Action::Keys { keys: "volumeup".into(), app: String::new() }),
                btn("b7", "음소거", "🔇", "#9b2c2c", "", Action::Keys { keys: "mute".into(), app: String::new() }),
                btn("b8", "구글", "🌐", "#2c7a7b", "", Action::Url { url: "https://www.google.com".into() }),
            ],
        }
    }
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("config.json"))
}

/// 자동 백업 폴더 (%APPDATA%\com.deck.streamdeck\backups)
pub fn backup_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?.join("backups");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// 남겨 둘 자동 백업 개수
const KEEP_BACKUPS: usize = 30;

/// "2026-11-08_14-03-22" (이 PC 시간 기준)
fn timestamp() -> String {
    use windows_sys::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::GetLocalTime};
    let mut t: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut t) };
    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}-{:03}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    )
}

/// 설정을 읽는다. 파일이 깨져 있으면 덮어쓰지 않도록 옆으로 옮겨 두고 기본값으로 시작하며, 알림 문구를 돌려준다.
pub fn load(app: &AppHandle) -> (Config, Option<String>) {
    let Ok(path) = config_path(app) else {
        return (Config::default(), None);
    };
    match read_with_retry(&path) {
        // 정말 처음 실행이라 파일이 없을 때만 기본 설정을 만든다
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let cfg = Config::default();
            let _ = save(app, &cfg);
            (cfg, None)
        }
        // 파일은 있는데 읽지 못함 (설치 직후 백신 검사, 다른 프로그램이 잠금 등).
        // 기본 설정으로 덮어쓰면 버튼이 사라진 것처럼 보이므로 파일은 절대 건드리지 않는다.
        Err(e) => (
            Config::default(),
            Some(format!(
                "설정 파일을 읽지 못해서 임시로 기본 버튼을 보여주고 있어요 ({e}). 버튼은 지워지지 않았어요. \
                 QuickDeck 을 종료했다가 다시 실행해 주세요. 그 전에는 편집·저장하지 않는 게 좋아요."
            )),
        ),
        Ok(text) => match parse(&text) {
            Ok(cfg) => (cfg, None),
            Err(e) => {
                let broken = path.with_file_name(format!("config.broken-{}.json", timestamp()));
                let _ = fs::rename(&path, &broken);
                let cfg = Config::default();
                let _ = save(app, &cfg);
                (
                    cfg,
                    Some(format!(
                        "설정 파일이 손상돼서 기본 설정으로 시작했어요 ({e}). 원래 파일은 {} 에 남겨 두었고, \
                         편집 → '설정 가져오기'로 backups 폴더의 이전 백업을 불러올 수 있어요.",
                        broken.display()
                    )),
                )
            }
        },
    }
}

/// 설정 파일 읽기. 잠깐 잠겨 있는 경우를 위해 몇 번 다시 시도한다 (없는 파일은 바로 NotFound)
fn read_with_retry(path: &std::path::Path) -> std::io::Result<String> {
    let mut last = None;
    for _ in 0..10 {
        match fs::read_to_string(path) {
            Ok(text) => return Ok(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(e),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
    }
    Err(last.unwrap())
}

/// JSON 문자열 → 설정 (가져오기에서도 사용)
pub fn parse(text: &str) -> Result<Config, String> {
    let mut cfg: Config = serde_json::from_str(text).map_err(|e| format!("QuickDeck 설정 형식이 아니에요: {e}"))?;
    cfg.columns = cfg.columns.clamp(1, 12);
    cfg.opacity = cfg.opacity.clamp(20, 100);
    Ok(cfg)
}

/// 저장: 이전 파일은 backups 폴더에 남기고, 임시 파일에 쓴 뒤 바꿔치기해서 저장 도중 꺼져도 파일이 깨지지 않게 한다.
pub fn save(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let path = config_path(app)?;
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    if path.is_file() {
        if let Ok(dir) = backup_dir(app) {
            let _ = fs::copy(&path, dir.join(format!("config-{}.json", timestamp())));
            prune_backups(&dir);
        }
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| format!("설정 저장 실패: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("설정 저장 실패: {e}"))
}

/// 오래된 자동 백업 지우기 (이름에 시각이 들어 있어 이름순 = 시간순)
fn prune_backups(dir: &std::path::Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("config-")))
        .collect();
    files.sort();
    let excess = files.len().saturating_sub(KEEP_BACKUPS);
    for old in &files[..excess] {
        let _ = fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_action_roundtrip() {
        let json = r#"{"type":"multi","steps":[
            {"type":"launch","target":"notepad.exe"},
            {"type":"delay","ms":1000},
            {"type":"text","text":"안녕하세요"},
            {"type":"keys","keys":"ctrl+s"}
        ]}"#;
        let action: Action = serde_json::from_str(json).unwrap();
        let Action::Multi { steps } = &action else { panic!("multi 아님") };
        assert_eq!(steps.len(), 4);
        assert!(matches!(steps[1], Action::Delay { ms: 1000 }));
        let back = serde_json::to_string(&action).unwrap();
        assert!(back.contains(r#""type":"delay""#));
    }

    #[test]
    fn prune_keeps_newest_backups() {
        let dir = std::env::temp_dir().join(format!("qd-backup-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for i in 0..(KEEP_BACKUPS + 5) {
            fs::write(dir.join(format!("config-2026-01-01_00-00-{i:02}-000.json")), "{}").unwrap();
        }
        fs::write(dir.join("other.txt"), "x").unwrap(); // 백업이 아닌 파일은 건드리지 않음
        prune_backups(&dir);
        let mut left: Vec<String> =
            fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left.len(), KEEP_BACKUPS + 1);
        assert!(left.contains(&"other.txt".to_string()));
        assert!(!left.contains(&"config-2026-01-01_00-00-00-000.json".to_string()), "가장 오래된 것은 지워져야");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_rejects_garbage_and_clamps_columns() {
        assert!(parse("이건 설정이 아님").is_err());
        assert!(parse(r#"{"foo":1}"#).is_err());
        let cfg = parse(r#"{"toggleHotkey":"","columns":99,"buttons":[]}"#).unwrap();
        assert_eq!(cfg.columns, 12);
    }

    #[test]
    fn nested_folders_flatten() {
        let json = r##"[
            {"id":"f1","label":"폴더","action":{"type":"folder","buttons":[
                {"id":"a","label":"A","hotkey":"Ctrl+Alt+KeyA","action":{"type":"keys","keys":"ctrl+c"}},
                {"id":"f2","label":"하위","action":{"type":"folder","buttons":[
                    {"id":"b","label":"B","action":{"type":"url","url":"https://example.com"}}
                ]}}
            ]}},
            {"id":"empty","label":"빈 폴더","action":{"type":"folder"}}
        ]"##;
        let buttons: Vec<Button> = serde_json::from_str(json).unwrap();
        let ids: Vec<_> = all_buttons(&buttons).iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["f1", "a", "f2", "b", "empty"]);
    }

    #[test]
    fn app_linked_folder_lookup() {
        let json = r##"[
            {"id":"hwp","label":"한글","action":{"type":"folder","apps":["Hwp.exe"],"buttons":[
                {"id":"save","label":"저장","action":{"type":"keys","keys":"alt+s"}},
                {"id":"sub","label":"서식","action":{"type":"folder","buttons":[
                    {"id":"bold","label":"진하게","action":{"type":"keys","keys":"ctrl+b"}},
                    {"id":"other","label":"메모장에","action":{"type":"keys","keys":"ctrl+v","app":"notepad.exe"}}
                ]}}
            ]}},
            {"id":"top","label":"위","action":{"type":"keys","keys":"ctrl+c"}}
        ]"##;
        let buttons: Vec<Button> = serde_json::from_str(json).unwrap();
        // 대소문자 무시하고 연결 폴더 찾기
        assert_eq!(folder_for_app(&buttons, "hwp.EXE").as_deref(), Some("hwp"));
        assert_eq!(folder_for_app(&buttons, "chrome.exe"), None);
        // 하위 폴더 안 버튼도 상위의 연결 프로그램을 물려받는다
        assert_eq!(find_button(&buttons, "bold").unwrap().1, ["Hwp.exe"]);
        assert_eq!(find_button(&buttons, "save").unwrap().1, ["Hwp.exe"]);
        assert!(find_button(&buttons, "top").unwrap().1.is_empty());
        // 버튼에 직접 지정한 대상은 Action 안에 그대로 남아 있다
        let (other, _) = find_button(&buttons, "other").unwrap();
        assert!(matches!(&other.action, Action::Keys { app, .. } if app == "notepad.exe"));
    }

    #[test]
    fn multi_app_folder_inherits_all_apps() {
        // 웹 브라우저처럼 여러 프로그램을 연결한 폴더: 첫 번째만이 아니라 전부 물려받아야 한다
        let json = r##"[
            {"id":"web","label":"웹","action":{"type":"folder","apps":["chrome.exe","msedge.exe","whale.exe"],"buttons":[
                {"id":"tab","label":"새 탭","action":{"type":"keys","keys":"ctrl+t"}}
            ]}}
        ]"##;
        let buttons: Vec<Button> = serde_json::from_str(json).unwrap();
        assert_eq!(find_button(&buttons, "tab").unwrap().1, ["chrome.exe", "msedge.exe", "whale.exe"]);
        assert_eq!(folder_for_app(&buttons, "Whale.exe").as_deref(), Some("web"));
    }

    #[test]
    fn old_config_still_loads() {
        // 멀티 액션 추가 전에 저장된 설정도 그대로 읽혀야 한다
        let json = r##"{"toggleHotkey":"Ctrl+Shift+Space","columns":6,"buttons":[
            {"id":"b1","label":"메모장","icon":"📝","color":"#2b6cb0","hotkey":"",
             "action":{"type":"launch","target":"notepad.exe","args":""}}]}"##;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.buttons.len(), 1);
    }
}

#[cfg(test)]
mod manual_parse {
    /// 실제 설정 파일이 읽히는지: QD_CONFIG=경로 cargo test --lib parse_real_config -- --ignored --nocapture
    #[test]
    #[ignore]
    fn parse_real_config() {
        let p = std::env::var("QD_CONFIG").expect("QD_CONFIG");
        let text = std::fs::read_to_string(&p).unwrap();
        match super::parse(&text) {
            Ok(c) => println!("OK: 최상위 버튼 {}개, 전체 {}개", c.buttons.len(), super::all_buttons(&c.buttons).len()),
            Err(e) => println!("FAIL: {e}"),
        }
    }
}

#[cfg(test)]
mod read_tests {
    use super::read_with_retry;

    #[test]
    fn missing_file_is_not_found_immediately() {
        let p = std::env::temp_dir().join("qd-no-such-config-file.json");
        let start = std::time::Instant::now();
        let e = read_with_retry(&p).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
        assert!(start.elapsed().as_millis() < 200, "없는 파일은 기다리지 않아야 함");
    }

    #[test]
    fn reads_existing_file() {
        let p = std::env::temp_dir().join(format!("qd-read-test-{}.json", std::process::id()));
        std::fs::write(&p, "{}").unwrap();
        assert_eq!(read_with_retry(&p).unwrap(), "{}");
        let _ = std::fs::remove_file(&p);
    }
}

#[cfg(test)]
mod toggle_tests {
    use super::*;

    #[test]
    fn toggle_and_long_press_roundtrip() {
        let json = r#"{"id":"mic","label":"마이크 끄기","action":{"type":"toggle",
            "first":{"type":"keys","keys":"ctrl+shift+m"},
            "second":{"type":"keys","keys":"ctrl+shift+m"},
            "altLabel":"마이크 켜기","altIcon":"🎙"},
            "longAction":{"type":"url","url":"https://example.com"}}"#;
        let b: Button = serde_json::from_str(json).unwrap();
        assert!(matches!(&b.action, Action::Toggle { alt_label, .. } if alt_label == "마이크 켜기"));
        assert!(matches!(&b.long_action, Some(Action::Url { .. })));
        let back = serde_json::to_string(&b).unwrap();
        assert!(back.contains("\"longAction\"") && back.contains("\"altIcon\""));
        // 길게 누르기가 없으면 저장할 때 빠진다
        let plain: Button = serde_json::from_str(r#"{"id":"a","label":"a","action":{"type":"url","url":"x"}}"#).unwrap();
        assert!(!serde_json::to_string(&plain).unwrap().contains("longAction"));
    }
}
