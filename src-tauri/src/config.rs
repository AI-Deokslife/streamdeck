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
    Keys { keys: String },
    /// 웹 주소 열기
    Url { url: String },
    /// 텍스트 입력 (스니펫)
    Text { text: String },
    /// PowerShell 명령 실행
    Script { command: String },
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
                btn("b3", "작업 관리자", "📊", "#c05621", "", Action::Keys { keys: "ctrl+shift+esc".into() }),
                btn("b4", "화면 캡처", "✂️", "#6b46c1", "", Action::Keys { keys: "win+shift+s".into() }),
                btn("b5", "볼륨 -", "🔉", "#4a5568", "Ctrl+Alt+Minus", Action::Keys { keys: "volumedown".into() }),
                btn("b6", "볼륨 +", "🔊", "#4a5568", "Ctrl+Alt+Equal", Action::Keys { keys: "volumeup".into() }),
                btn("b7", "음소거", "🔇", "#9b2c2c", "", Action::Keys { keys: "mute".into() }),
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
