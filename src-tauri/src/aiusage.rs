//! Claude 사용량(현재 세션 5시간 · 이번 주 7일)을 QuickDeck 버튼에 보여 주기.
//!
//! Claude Code 는 상태 표시줄(statusLine) 명령에 사용량 정보(rate_limits)를 JSON 으로 넘겨준다.
//! `quickdeck.exe --statusline` 이 그 명령이 되어 숫자를 파일에 적어 두고, 버튼은 그 파일을 읽는다.
//! 원래 쓰던 상태 표시줄이 있으면 그대로 이어서 실행해 화면은 예전과 똑같이 보인다.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

/// 사용량 창 하나 (사용률 %와 초기화 시각)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub pct: f64,
    /// 초기화되는 시각 (유닉스 초)
    pub resets_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    /// 현재 세션 (5시간)
    pub session: Option<Window>,
    /// 이번 주 (7일)
    pub week: Option<Window>,
    /// 이 숫자를 받은 시각 (유닉스 초)
    pub updated: u64,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// 숫자를 적어 두는 곳 (%USERPROFILE%\.quickdeck).
/// AppData 는 Claude 데스크톱 앱처럼 포장된 앱 안에서 실행되면 따로 복사본에 쓰여서, 어디서 실행해도 같은 사용자 폴더를 쓴다
fn data_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("QUICKDECK_DATA_DIR") {
        return Some(PathBuf::from(d));
    }
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".quickdeck"))
}

fn usage_file() -> Option<PathBuf> {
    data_dir().map(|d| d.join("ai-usage.json"))
}

/// 연결 전에 쓰던 상태 표시줄 명령 (이어서 실행하고, 연결을 끊으면 되돌린다)
fn chain_file() -> Option<PathBuf> {
    data_dir().map(|d| d.join("statusline-chain.json"))
}

/// Claude Code 설정 파일 (~/.claude/settings.json, CLAUDE_CONFIG_DIR 이 있으면 그 안)
fn claude_settings() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir).join("settings.json"));
    }
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".claude").join("settings.json"))
}

/// 상태 표시줄 JSON → 사용량. rate_limits 가 없으면 None (구독 계정이 아니거나 아직 첫 응답 전)
pub fn parse_status(v: &Value, now: u64) -> Option<Usage> {
    let rl = v.get("rate_limits")?;
    let win = |key: &str| -> Option<Window> {
        let w = rl.get(key)?;
        let pct = w.get("used_percentage")?.as_f64()?;
        let resets_at = w.get("resets_at").and_then(|r| r.as_u64().or_else(|| r.as_f64().map(|f| f as u64))).unwrap_or(0);
        Some(Window { pct: pct.clamp(0.0, 100.0), resets_at })
    };
    let (session, week) = (win("five_hour"), win("seven_day"));
    if session.is_none() && week.is_none() {
        return None;
    }
    Some(Usage { session, week, updated: now })
}

/// 버튼에 보여 줄 사용량 (없으면 None = 아직 연결 안 됨)
pub fn read() -> Option<Usage> {
    let text = fs::read_to_string(usage_file()?).ok()?;
    serde_json::from_str(&text).ok()
}

fn save(u: &Usage) {
    let (Some(dir), Some(path)) = (data_dir(), usage_file()) else { return };
    let _ = fs::create_dir_all(dir);
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, serde_json::to_string(u).unwrap_or_default()).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

/// 원래 상태 표시줄 명령을 같은 입력으로 실행하고 그 출력을 돌려준다
fn run_chained(command: &str, input: &str) -> Option<String> {
    // Claude Code 는 Windows 에서 Git Bash 로 명령을 실행하므로 가능하면 똑같이 bash 로
    let bash = [r"C:\Program Files\Git\bin\bash.exe", r"C:\Program Files (x86)\Git\bin\bash.exe"]
        .into_iter()
        .find(|p| std::path::Path::new(p).exists());
    let mut cmd = match bash {
        Some(b) => {
            let mut c = Command::new(b);
            c.args(["-c", command]);
            c
        }
        None => {
            let mut c = Command::new("cmd.exe");
            c.args(["/C", command]);
            c
        }
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    if let Some(mut si) = child.stdin.take() {
        let _ = si.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn pct_text(w: &Option<Window>) -> String {
    w.as_ref().map(|w| format!("{:.0}%", w.pct)).unwrap_or_else(|| "–".into())
}

/// `quickdeck.exe --statusline`: Claude Code 가 상태 표시줄을 그릴 때마다 실행된다. 종료 코드를 돌려준다
pub fn statusline_main() -> i32 {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let parsed: Value = serde_json::from_str(&input).unwrap_or(Value::Null);
    let usage = parse_status(&parsed, now_secs());
    if let Some(u) = &usage {
        save(u);
    }
    let chained = chain_file()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("command").and_then(|c| c.as_str()).map(String::from))
        .filter(|c| !c.trim().is_empty());
    let line = match chained {
        Some(c) => run_chained(&c, &input).unwrap_or_default(),
        None => match &usage {
            Some(u) => format!("세션 {} · 주간 {}", pct_text(&u.session), pct_text(&u.week)),
            None => String::new(),
        },
    };
    let mut out = std::io::stdout();
    let _ = out.write_all(line.as_bytes());
    let _ = out.flush();
    0
}

// ---------------- Claude Code 와 연결하기 / 끊기 ----------------

/// 상태 표시줄 명령 (Claude Code 가 bash 로 실행하므로 경로는 / 로)
fn our_command(exe: &std::path::Path) -> String {
    format!("\"{}\" --statusline", exe.display().to_string().replace('\\', "/"))
}

fn is_ours(command: &str) -> bool {
    command.contains("--statusline") && command.to_lowercase().contains("quickdeck")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkStatus {
    /// Claude Code 설정에 연결돼 있음
    pub connected: bool,
    /// Claude Code 설정 폴더가 있음 (설치돼 있음)
    pub claude_found: bool,
    pub usage: Option<Usage>,
}

fn read_settings(path: &std::path::Path) -> Result<Value, String> {
    match fs::read_to_string(path) {
        Ok(t) if t.trim().is_empty() => Ok(json!({})),
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("Claude Code 설정 파일을 읽지 못했어요 ({e}). 파일을 고치지 않았어요")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("Claude Code 설정 파일을 열지 못했어요: {e}")),
    }
}

fn write_settings(path: &std::path::Path, v: &Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if path.is_file() {
        let _ = fs::copy(path, path.with_extension("json.quickdeck-backup"));
    }
    let text = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text + "\n").map_err(|e| format!("Claude Code 설정을 저장하지 못했어요: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("Claude Code 설정을 저장하지 못했어요: {e}"))
}

pub fn status() -> LinkStatus {
    let settings = claude_settings();
    let claude_found = settings.as_ref().and_then(|p| p.parent()).is_some_and(|d| d.is_dir());
    let connected = settings
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.pointer("/statusLine/command").and_then(|c| c.as_str()).map(is_ours))
        .unwrap_or(false);
    LinkStatus { connected, claude_found, usage: read() }
}

/// Claude Code 의 상태 표시줄을 QuickDeck 으로 연결 (원래 명령은 기억해 두고 이어서 실행)
pub fn connect(exe: &std::path::Path) -> Result<(), String> {
    let path = claude_settings().ok_or("사용자 폴더를 찾지 못했어요")?;
    let mut v = read_settings(&path)?;
    if !v.is_object() {
        return Err("Claude Code 설정 파일 모양이 예상과 달라요. 파일을 고치지 않았어요".into());
    }
    let old = v.pointer("/statusLine/command").and_then(|c| c.as_str()).map(String::from);
    if let Some(old) = old.filter(|c| !is_ours(c)) {
        let dir = data_dir().ok_or("설정 폴더를 찾지 못했어요")?;
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let keep = json!({ "command": old, "statusLine": v.get("statusLine").cloned() });
        fs::write(chain_file().unwrap(), serde_json::to_string_pretty(&keep).unwrap()).map_err(|e| e.to_string())?;
    }
    let mut line = v.get("statusLine").cloned().filter(|s| s.is_object()).unwrap_or_else(|| json!({}));
    line["type"] = json!("command");
    line["command"] = json!(our_command(exe));
    v["statusLine"] = line;
    write_settings(&path, &v)
}

/// 연결 끊기: 원래 상태 표시줄로 되돌리거나(있었다면) 지운다
pub fn disconnect() -> Result<(), String> {
    let path = claude_settings().ok_or("사용자 폴더를 찾지 못했어요")?;
    let mut v = read_settings(&path)?;
    let ours = v.pointer("/statusLine/command").and_then(|c| c.as_str()).is_some_and(is_ours);
    if ours {
        let saved = chain_file()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|k| k.get("statusLine").cloned())
            .filter(|s| s.is_object());
        match saved {
            Some(s) => v["statusLine"] = s,
            None => {
                if let Some(o) = v.as_object_mut() {
                    o.remove("statusLine");
                }
            }
        }
        write_settings(&path, &v)?;
    }
    if let Some(p) = chain_file() {
        let _ = fs::remove_file(p);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rate_limits() {
        let v: Value = serde_json::from_str(
            r#"{"model":{"id":"x"},"rate_limits":{"five_hour":{"used_percentage":26.4,"resets_at":1763322600},
                "seven_day":{"used_percentage":25,"resets_at":1763722800}}}"#,
        )
        .unwrap();
        let u = parse_status(&v, 100).unwrap();
        assert_eq!(u.session, Some(Window { pct: 26.4, resets_at: 1763322600 }));
        assert_eq!(u.week.as_ref().unwrap().pct, 25.0);
        assert_eq!(u.updated, 100);
        // 한쪽만 있어도 된다 (Claude Code 는 지난 창을 빼고 보낸다)
        let one: Value = serde_json::from_str(r#"{"rate_limits":{"seven_day":{"used_percentage":3,"resets_at":5}}}"#).unwrap();
        let u = parse_status(&one, 1).unwrap();
        assert!(u.session.is_none() && u.week.is_some());
        // 없으면 None (파일을 덮어쓰지 않는다)
        assert!(parse_status(&serde_json::from_str(r#"{"model":{}}"#).unwrap(), 1).is_none());
        assert!(parse_status(&serde_json::from_str(r#"{"rate_limits":{}}"#).unwrap(), 1).is_none());
    }

    #[test]
    fn connect_keeps_and_restores_old_statusline() {
        // 임시 폴더에서: 원래 상태 표시줄 + 다른 설정이 있는 settings.json
        let root = std::env::temp_dir().join(format!("qd-aiusage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("claude")).unwrap();
        unsafe {
            std::env::set_var("CLAUDE_CONFIG_DIR", root.join("claude"));
            std::env::set_var("QUICKDECK_DATA_DIR", root.join("data"));
        }
        let settings = root.join("claude").join("settings.json");
        fs::write(&settings, r#"{"model":"opus","statusLine":{"type":"command","command":"bash ~/my.sh","padding":1},"zeta":1}"#).unwrap();
        let exe = std::path::Path::new(r"C:\Apps\QuickDeck\quickdeck.exe");
        connect(exe).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        assert!(is_ours(v["statusLine"]["command"].as_str().unwrap()));
        assert_eq!(v["statusLine"]["padding"], 1, "다른 상태 표시줄 설정은 그대로");
        assert_eq!(v["model"], "opus");
        let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["model", "statusLine", "zeta"], "키 순서도 그대로");
        assert!(status().connected);
        // 두 번 연결해도 원래 명령을 잃지 않는다
        connect(exe).unwrap();
        disconnect().unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(v["statusLine"]["command"], "bash ~/my.sh", "원래 상태 표시줄로 되돌린다");
        assert!(!status().connected);
        // 원래 상태 표시줄이 없던 경우: 끊으면 statusLine 자체를 지운다
        fs::write(&settings, r#"{"model":"opus"}"#).unwrap();
        connect(exe).unwrap();
        disconnect().unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        assert!(v.get("statusLine").is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn command_is_recognized() {
        let c = our_command(std::path::Path::new(r"C:\Users\선생님\AppData\Local\QuickDeck\quickdeck.exe"));
        assert_eq!(c, "\"C:/Users/선생님/AppData/Local/QuickDeck/quickdeck.exe\" --statusline");
        assert!(is_ours(&c));
        assert!(!is_ours("bash ~/.claude/statusline.sh"));
    }
}
