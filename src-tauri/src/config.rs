//! 설정 파일(config.json) 구조와 로드/저장.
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

/// 버튼이 실행하는 동작.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
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
    },
    /// PowerShell 명령 실행
    Script { command: String },
    /// 기다리기 (멀티 액션의 단계로 사용)
    Delay { ms: u64 },
    /// 여러 동작을 순서대로 실행
    Multi { steps: Vec<Action> },
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
    #[serde(default = "default_color")]
    pub color: String,
    /// 이 버튼을 바로 실행하는 전역 단축키 (선택)
    #[serde(default)]
    pub hotkey: String,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// 창 보이기/숨기기 전역 단축키
    pub toggle_hotkey: String,
    pub columns: u32,
    pub buttons: Vec<Button>,
}

fn default_color() -> String {
    "#2d3748".into()
}

fn btn(id: &str, label: &str, icon: &str, color: &str, hotkey: &str, action: Action) -> Button {
    Button {
        id: id.into(),
        label: label.into(),
        icon: icon.into(),
        color: color.into(),
        hotkey: hotkey.into(),
        action,
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            toggle_hotkey: "Ctrl+Shift+F12".into(),
            columns: 4,
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

pub fn load(app: &AppHandle) -> Config {
    let Ok(path) = config_path(app) else {
        return Config::default();
    };
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("config.json 파싱 실패, 기본값 사용: {e}");
            Config::default()
        }),
        Err(_) => {
            let cfg = Config::default();
            let _ = save(app, &cfg);
            cfg
        }
    }
}

pub fn save(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let path = config_path(app)?;
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
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
