//! 휴대폰 리모컨: 같은 와이파이의 휴대폰 브라우저에서 QuickDeck 버튼을 누를 수 있게 하는 작은 웹 서버.
//!
//! - 기본은 꺼짐. 켜면 0.0.0.0:<port> 에서 기다린다 (처음 한 번 Windows 방화벽 허용 창이 뜬다).
//! - 모든 API 는 비밀 열쇠(token)가 맞아야 동작한다. 열쇠는 QR 코드 주소에 들어 있다.
//! - 휴대폰 페이지는 1.5초마다 버튼 목록이 바뀌었는지 확인한다 (PC 에서 고치면 바로 반영).
use crate::config;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::Read,
    net::UdpSocket,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
};
use tauri::{AppHandle, Manager};
use tiny_http::{Header, Method, Request, Response, Server};

const DEFAULT_PORT: u16 = 47823;
/// 휴대폰에서 접속할 수 있게 모든 네트워크에서 기다린다. 테스트 때는 이 PC 안에서만 (방화벽 창이 뜨지 않게)
#[cfg(not(test))]
const BIND: &str = "0.0.0.0";
#[cfg(test)]
const BIND: &str = "127.0.0.1";
const PAGE: &str = include_str!("remote.html");
const ICON: &[u8] = include_bytes!("../icons/128x128@2x.png");

/// remote.json (설정 파일과 따로 둔다 — 화면에서 저장하는 config.json 에 덮어써지지 않게)
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSettings {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
}

impl Default for RemoteSettings {
    fn default() -> Self {
        Self { enabled: false, port: DEFAULT_PORT, token: new_token() }
    }
}

#[derive(Default)]
pub struct RemoteState {
    server: Mutex<Option<Arc<Server>>>,
    settings: Mutex<Option<RemoteSettings>>,
    error: Mutex<Option<String>>,
    /// 버튼 설정이 바뀔 때마다 1씩 올린다 (휴대폰이 새로 받아야 하는지 판단)
    version: AtomicU64,
}

/// 화면에 보여줄 상태
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    pub enabled: bool,
    pub running: bool,
    pub url: String,
    pub qr_svg: String,
    pub error: Option<String>,
}

// ---------------- 설정 파일 ----------------

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("remote.json"))
}

fn load_settings(app: &AppHandle) -> RemoteSettings {
    let state = app.state::<RemoteState>();
    let mut guard = state.settings.lock().unwrap();
    if let Some(s) = guard.as_ref() {
        return s.clone();
    }
    let s = settings_path(app)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<RemoteSettings>(&t).ok())
        .filter(|s| s.token.len() >= 16)
        .unwrap_or_default();
    *guard = Some(s.clone());
    s
}

fn save_settings(app: &AppHandle, s: &RemoteSettings) {
    *app.state::<RemoteState>().settings.lock().unwrap() = Some(s.clone());
    if let Some(p) = settings_path(app) {
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(p, serde_json::to_string_pretty(s).unwrap_or_default());
    }
}

/// 운영체제 난수로 32자리 열쇠 만들기
fn new_token() -> String {
    use windows_sys::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
    let mut buf = [0u8; 16];
    unsafe { BCryptGenRandom(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// 같은 공유기 안에서 이 PC 의 주소 (예: 192.168.0.12). 실제로 패킷을 보내지는 않는다.
fn lan_ip() -> String {
    for probe in ["8.8.8.8:80", "192.168.0.1:80", "10.0.0.1:80"] {
        if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
            if sock.connect(probe).is_ok() {
                if let Ok(addr) = sock.local_addr() {
                    let ip = addr.ip();
                    if !ip.is_loopback() && !ip.is_unspecified() {
                        return ip.to_string();
                    }
                }
            }
        }
    }
    "127.0.0.1".into()
}

// ---------------- 켜기 / 끄기 / 상태 ----------------

pub fn status(app: &AppHandle) -> RemoteStatus {
    let s = load_settings(app);
    let state = app.state::<RemoteState>();
    let running = state.server.lock().unwrap().is_some();
    let url = format!("http://{}:{}/?t={}", lan_ip(), s.port, s.token);
    let qr_svg = if running { qr_svg(&url) } else { String::new() };
    RemoteStatus { enabled: s.enabled, running, url, qr_svg, error: state.error.lock().unwrap().clone() }
}

fn qr_svg(text: &str) -> String {
    use qrcode::{render::svg, QrCode};
    QrCode::new(text.as_bytes())
        .map(|c| {
            c.render::<svg::Color>()
                .min_dimensions(220, 220)
                .dark_color(svg::Color("#111111"))
                .light_color(svg::Color("#ffffff"))
                .build()
        })
        .unwrap_or_default()
}

pub fn set_enabled(app: &AppHandle, enabled: bool) -> RemoteStatus {
    let mut s = load_settings(app);
    s.enabled = enabled;
    save_settings(app, &s);
    stop(app);
    if enabled {
        start(app);
    }
    status(app)
}

/// 열쇠를 새로 만든다 (이전에 연결한 휴대폰은 더 이상 조작할 수 없다)
pub fn regenerate(app: &AppHandle) -> RemoteStatus {
    let mut s = load_settings(app);
    s.token = new_token();
    save_settings(app, &s);
    status(app)
}

/// 앱 시작 시: 켜져 있었으면 다시 켠다
pub fn start_if_enabled(app: &AppHandle) {
    if load_settings(app).enabled {
        start(app);
    }
}

/// 버튼 설정이 바뀌었음을 휴대폰에 알린다
pub fn bump_version(app: &AppHandle) {
    app.state::<RemoteState>().version.fetch_add(1, Ordering::SeqCst);
}

fn stop(app: &AppHandle) {
    if let Some(server) = app.state::<RemoteState>().server.lock().unwrap().take() {
        server.unblock();
    }
}

fn start(app: &AppHandle) {
    let mut s = load_settings(app);
    let state = app.state::<RemoteState>();
    // 정해진 포트가 쓰이고 있으면 다음 번호들을 시도하고, 성공한 번호를 기억한다
    let mut last_err = String::new();
    for port in s.port..s.port.saturating_add(10) {
        match Server::http((BIND, port)) {
            Ok(server) => {
                if port != s.port {
                    s.port = port;
                    save_settings(app, &s);
                }
                let server = Arc::new(server);
                *state.server.lock().unwrap() = Some(server.clone());
                *state.error.lock().unwrap() = None;
                let host: Arc<dyn Host> = Arc::new(AppHost(app.clone()));
                thread::spawn(move || serve(host, server));
                return;
            }
            Err(e) => last_err = e.to_string(),
        }
    }
    *state.error.lock().unwrap() = Some(format!("휴대폰 리모컨을 켜지 못했어요: {last_err}"));
}

// ---------------- 요청 처리 ----------------

/// 서버가 앱에서 필요한 것 (테스트에서는 가짜로 바꿔 끼운다)
trait Host: Send + Sync + 'static {
    fn token(&self) -> String;
    fn version(&self) -> u64;
    fn deck(&self, version: u64) -> Value;
    fn press(&self, id: &str, long: bool) -> Result<(), String>;
    /// 지금 PC 에서 앞에 있는 프로그램에 연결된 폴더 (휴대폰도 따라서 자동 전환)
    fn active_folder(&self) -> Option<String>;
    /// 트랙패드·키보드 탭 조작
    fn control(&self, c: Control) -> Result<(), String>;
    /// 음악 탭: 지금 재생 중인 곡과 볼륨
    fn media(&self) -> Value;
    /// 위젯 버튼 값 (CPU·메모리·타이머)
    fn widgets(&self) -> Value;
}

/// 휴대폰 트랙패드·키보드 탭에서 오는 조작
#[derive(Debug, PartialEq)]
pub enum Control {
    Move(i32, i32),
    Click(&'static str),
    Scroll(i32),
    Type(String),
    Key(String),
    /// "toggle" / "next" / "prev"
    Media(&'static str),
    Volume(i32),
    Mute,
}

/// 쿼리/본문 → 조작. 값이 이상하면 None
fn parse_control(op: &str, get: &dyn Fn(&str) -> Option<String>, body: &str) -> Option<Control> {
    let num = |k: &str| get(k).and_then(|v| v.parse::<i32>().ok()).map(|v| v.clamp(-2000, 2000));
    match op {
        "move" => Some(Control::Move(num("dx")?, num("dy")?)),
        "click" => match get("btn").as_deref() {
            Some("right") => Some(Control::Click("right")),
            Some("middle") => Some(Control::Click("middle")),
            Some("double") => Some(Control::Click("double")),
            _ => Some(Control::Click("left")),
        },
        "scroll" => Some(Control::Scroll(num("dy")?)),
        "type" if !body.is_empty() => Some(Control::Type(body.chars().take(5000).collect())),
        // 키 이름은 단축키 버튼과 같은 형식 (enter, ctrl+c ...). 글자·숫자·+·공백만 허용
        "key" => get("k").filter(|k| !k.is_empty() && k.len() <= 40 && k.chars().all(|c| c.is_ascii_alphanumeric() || "+ ".contains(c))).map(Control::Key),
        "media" => Some(Control::Media(match get("cmd").as_deref() {
            Some("next") => "next",
            Some("prev") => "prev",
            _ => "toggle",
        })),
        "volume" => num("v").map(|v| Control::Volume(v.clamp(0, 100))),
        "mute" => Some(Control::Mute),
        _ => None,
    }
}

struct AppHost(AppHandle);

impl Host for AppHost {
    fn token(&self) -> String {
        load_settings(&self.0).token
    }
    fn version(&self) -> u64 {
        self.0.state::<RemoteState>().version.load(Ordering::SeqCst)
    }
    fn deck(&self, version: u64) -> Value {
        let state = self.0.state::<crate::AppState>();
        let cfg = state.config.lock().unwrap().clone();
        let toggles = state.toggles.lock().unwrap().clone();
        deck_json(&cfg, version, &toggles)
    }
    fn press(&self, id: &str, long: bool) -> Result<(), String> {
        crate::press_from_remote(&self.0, id, long)
    }
    fn active_folder(&self) -> Option<String> {
        let exe = crate::winutil::last_app();
        let state = self.0.state::<crate::AppState>();
        let cfg = state.config.lock().unwrap();
        config::folder_for_app(&cfg.buttons, &exe)
    }
    fn control(&self, c: Control) -> Result<(), String> {
        use crate::actions;
        match c {
            Control::Move(dx, dy) => actions::mouse_move(dx, dy),
            Control::Click(b) => actions::mouse_click(b),
            Control::Scroll(dy) => actions::mouse_scroll(dy),
            Control::Type(text) => actions::type_from_remote(&text),
            Control::Key(k) => actions::send_keys(&k),
            Control::Media(cmd) => crate::media::media_command(cmd),
            Control::Volume(v) => crate::media::set_volume(v),
            Control::Mute => crate::media::toggle_mute(),
        }
    }
    fn widgets(&self) -> Value {
        json!(crate::widgets::snapshot())
    }
    fn media(&self) -> Value {
        let (volume, muted) = crate::media::volume().unwrap_or((0, false));
        json!({ "now": crate::media::now_playing(), "volume": volume, "muted": muted })
    }
}

fn serve(host: Arc<dyn Host>, server: Arc<Server>) {
    for req in server.incoming_requests() {
        let host = host.clone();
        // 버튼 동작(멀티 액션의 기다리기 등)이 다른 요청을 막지 않도록 요청마다 따로 처리
        thread::spawn(move || handle(&*host, req));
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("올바른 헤더")
}

fn query(url: &str) -> Vec<(String, String)> {
    url.split_once('?')
        .map(|(_, q)| {
            q.split('&')
                .filter_map(|kv| kv.split_once('='))
                .map(|(k, v)| (k.to_string(), percent_decode(v)))
                .collect()
        })
        .unwrap_or_default()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 길이가 같으면 끝까지 비교 (열쇠를 한 글자씩 맞혀 보는 공격을 어렵게)
fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn respond_json(req: Request, status: u16, body: Value) {
    let _ = req.respond(
        Response::from_string(body.to_string())
            .with_status_code(status)
            .with_header(header("Content-Type", "application/json; charset=utf-8"))
            .with_header(header("Cache-Control", "no-store")),
    );
}

fn handle(host: &dyn Host, mut req: Request) {
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or("/").to_string();
    let q = query(&url);
    let get = |k: &str| q.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str());

    match (req.method(), path.as_str()) {
        // 페이지와 아이콘은 열쇠 없이 받을 수 있다 (페이지 안에는 비밀이 없다)
        (Method::Get, "/") => {
            let _ = req.respond(
                Response::from_string(PAGE)
                    .with_header(header("Content-Type", "text/html; charset=utf-8"))
                    .with_header(header("Cache-Control", "no-store")),
            );
            return;
        }
        (Method::Get, "/icon.png") => {
            let _ = req.respond(Response::from_data(ICON).with_header(header("Content-Type", "image/png")));
            return;
        }
        (Method::Get, "/manifest.json") => {
            let token = get("t").unwrap_or("");
            let manifest = json!({
                "name": "QuickDeck 리모컨", "short_name": "QuickDeck",
                "start_url": format!("/?t={token}"), "display": "standalone",
                "background_color": "#000000", "theme_color": "#000000",
                "icons": [{ "src": "/icon.png", "sizes": "256x256", "type": "image/png" }]
            });
            respond_json(req, 200, manifest);
            return;
        }
        _ => {}
    }

    // 여기부터는 열쇠 확인
    let token = host.token();
    if !get("t").is_some_and(|t| same_token(t, &token)) {
        respond_json(req, 403, json!({ "ok": false, "error": "열쇠가 맞지 않아요. PC 의 QR 코드를 다시 찍어 주세요" }));
        return;
    }

    match (req.method(), path.as_str()) {
        (Method::Get, "/api/deck") => {
            let version = host.version();
            let active = host.active_folder();
            if get("v").and_then(|v| v.parse::<u64>().ok()) == Some(version) {
                respond_json(req, 200, json!({ "changed": false, "version": version, "activeFolder": active }));
                return;
            }
            let mut deck = host.deck(version);
            deck["activeFolder"] = json!(active);
            respond_json(req, 200, deck);
        }
        (Method::Post, "/api/press") => {
            // 본문은 쓰지 않지만 읽어서 비운다
            let mut _body = String::new();
            let _ = req.as_reader().take(4096).read_to_string(&mut _body);
            let Some(id) = get("id").map(str::to_string) else {
                respond_json(req, 400, json!({ "ok": false, "error": "버튼 id 가 없어요" }));
                return;
            };
            match host.press(&id, get("long") == Some("1")) {
                Ok(()) => respond_json(req, 200, json!({ "ok": true })),
                Err(e) => respond_json(req, 200, json!({ "ok": false, "error": e })),
            }
        }
        (Method::Get, "/api/media") => respond_json(req, 200, host.media()),
        (Method::Get, "/api/widgets") => respond_json(req, 200, host.widgets()),
        (Method::Post, "/api/control") => {
            let mut body = String::new();
            let _ = req.as_reader().take(20_000).read_to_string(&mut body);
            let getter = |k: &str| get(k).map(str::to_string);
            let Some(c) = parse_control(get("op").unwrap_or(""), &getter, &body) else {
                respond_json(req, 400, json!({ "ok": false, "error": "알 수 없는 조작이에요" }));
                return;
            };
            match host.control(c) {
                Ok(()) => respond_json(req, 200, json!({ "ok": true })),
                Err(e) => respond_json(req, 200, json!({ "ok": false, "error": e })),
            }
        }
        _ => respond_json(req, 404, json!({ "ok": false, "error": "없는 주소예요" })),
    }
}

/// 휴대폰에 보낼 버튼 목록. 실행 경로·단축키·명령 같은 내용은 빼고 모양만 보낸다.
fn deck_json(cfg: &config::Config, version: u64, toggles: &std::collections::HashMap<String, bool>) -> Value {
    fn strip(list: &[config::Button], toggles: &std::collections::HashMap<String, bool>) -> Vec<Value> {
        list.iter()
            .map(|b| {
                let (kind, children) = match &b.action {
                    config::Action::Folder { buttons, .. } => ("folder", strip(buttons, toggles)),
                    config::Action::Multi { .. } => ("multi", vec![]),
                    config::Action::Toggle { .. } => ("toggle", vec![]),
                    config::Action::Clock {} => ("clock", vec![]),
                    config::Action::Timer { .. } => ("timer", vec![]),
                    config::Action::SysInfo {} => ("sysinfo", vec![]),
                    _ => ("button", vec![]),
                };
                let (alt_label, alt_icon) = match &b.action {
                    config::Action::Toggle { alt_label, alt_icon, .. } => (alt_label.as_str(), alt_icon.as_str()),
                    _ => ("", ""),
                };
                json!({
                    "id": b.id, "label": b.label, "icon": b.icon, "color": b.color,
                    "image": b.image, "imageFull": b.image_full, "kind": kind, "children": children,
                    "on": toggles.get(&b.id).copied().unwrap_or(false),
                    "altLabel": alt_label, "altIcon": alt_icon,
                    "hasLong": b.long_action.is_some() || matches!(b.action, config::Action::Timer { .. }),
                    "minutes": match &b.action { config::Action::Timer { minutes } => *minutes, _ => 0 },
                })
            })
            .collect()
    }
    json!({
        "changed": true,
        "version": version,
        "columns": cfg.columns,
        "theme": cfg.theme,
        "buttons": strip(&cfg.buttons, toggles),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_random_hex() {
        let a = new_token();
        let b = new_token();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn token_comparison() {
        assert!(same_token("abc123", "abc123"));
        assert!(!same_token("abc123", "abc124"));
        assert!(!same_token("abc", "abc123"));
    }

    #[test]
    fn parses_query() {
        let q = query("/api/press?t=abc&id=b%201&x=a+b");
        assert_eq!(q[0], ("t".into(), "abc".into()));
        assert_eq!(q[1], ("id".into(), "b 1".into()));
        assert_eq!(q[2], ("x".into(), "a b".into()));
        assert!(query("/").is_empty());
    }

    #[test]
    fn deck_hides_commands() {
        let cfg: config::Config = serde_json::from_str(
            r#"{"toggleHotkey":"","columns":4,"buttons":[
                {"id":"s","label":"비밀","action":{"type":"script","command":"secret-command"}},
                {"id":"f","label":"폴더","action":{"type":"folder","apps":["Hwp.exe"],"buttons":[
                    {"id":"k","label":"저장","action":{"type":"keys","keys":"alt+s"}}]}}]}"#,
        )
        .unwrap();
        let text = deck_json(&cfg, 3, &Default::default()).to_string();
        assert!(!text.contains("secret-command"), "명령 내용이 휴대폰으로 가면 안 됨");
        assert!(!text.contains("alt+s"));
        assert!(!text.contains("Hwp.exe"));
        assert!(text.contains("\"kind\":\"folder\""));
        assert!(text.contains("저장"));
    }

    /// 실제로 서버를 켜고 HTTP 로 확인 (이 PC 안에서만 접속, 방화벽 창 없음)
    #[test]
    fn server_end_to_end() {
        use std::io::{Read as _, Write as _};
        use std::net::TcpStream;

        struct TestHost {
            token: String,
            version: AtomicU64,
            cfg: config::Config,
            controls: Mutex<Vec<String>>,
        }
        impl Host for TestHost {
            fn token(&self) -> String {
                self.token.clone()
            }
            fn version(&self) -> u64 {
                self.version.load(Ordering::SeqCst)
            }
            fn deck(&self, version: u64) -> Value {
                deck_json(&self.cfg, version, &Default::default())
            }
            fn press(&self, id: &str, _long: bool) -> Result<(), String> {
                if id == "f" { Ok(()) } else { Err("버튼을 찾을 수 없어요".into()) }
            }
            fn active_folder(&self) -> Option<String> {
                Some("f".into())
            }
            fn control(&self, c: Control) -> Result<(), String> {
                self.controls.lock().unwrap().push(format!("{c:?}"));
                Ok(())
            }
            fn widgets(&self) -> Value {
                json!({ "cpu": 37, "memory": 48, "timers": { "t": { "remainingMs": 183_000, "running": true, "done": false } } })
            }
            fn media(&self) -> Value {
                json!({ "now": { "title": "테스트 곡", "artist": "가수", "playing": true, "source": "x", "art": "" }, "volume": 40, "muted": false })
            }
        }
        let cfg: config::Config = serde_json::from_str(
            r##"{"toggleHotkey":"","columns":3,"theme":"toss","buttons":[
                {"id":"f","label":"폴더","action":{"type":"folder","buttons":[
                    {"id":"k","label":"저장","action":{"type":"keys","keys":"alt+s"}},
                    {"id":"c","label":"","color":"#1c1c1e","action":{"type":"clock"}},
                    {"id":"t","label":"라면","color":"#ff9f0a","action":{"type":"timer","minutes":5}},
                    {"id":"s","label":"PC 상태","color":"#5e5ce6","action":{"type":"sysInfo"}},
                    {"id":"w","label":"왼쪽 절반","icon":"◧","action":{"type":"window","mode":"left"}}]}}]}"##,
        )
        .unwrap();
        let token = new_token();
        let host = Arc::new(TestHost { token: token.clone(), version: AtomicU64::new(0), cfg, controls: Mutex::new(vec![]) });
        let (server, port) = (47900..47950)
            .find_map(|p| Server::http((BIND, p)).ok().map(|s| (Arc::new(s), p)))
            .expect("테스트 포트를 열지 못함");
        {
            let host: Arc<dyn Host> = host.clone();
            let server = server.clone();
            thread::spawn(move || serve(host, server));
        }

        let call = |method: &str, path: &str| -> (u16, String) {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            let code = out.split_whitespace().nth(1).unwrap().parse().unwrap();
            let body = out.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
            (code, body)
        };

        // 페이지는 열쇠 없이도 받는다
        let (code, page) = call("GET", "/");
        assert_eq!(code, 200);
        assert!(page.contains("QuickDeck 리모컨"));
        // 열쇠 없거나 틀리면 거절
        assert_eq!(call("GET", "/api/deck").0, 403);
        assert_eq!(call("GET", "/api/deck?t=wrongwrongwrongwrongwrongwrong12").0, 403);
        assert_eq!(call("POST", "/api/press?id=f").0, 403);
        // 맞는 열쇠면 버튼 목록 (명령 내용은 없음)
        let (code, body) = call("GET", &format!("/api/deck?t={token}"));
        assert_eq!(code, 200);
        assert!(body.contains("\"theme\":\"toss\"") && body.contains("저장"));
        assert!(!body.contains("alt+s"));
        let v: Value = serde_json::from_str(&body).unwrap();
        let ver = v["version"].as_u64().unwrap();
        // 바뀐 게 없으면 changed:false, 설정이 바뀌면 다시 받음
        assert!(call("GET", &format!("/api/deck?t={token}&v={ver}")).1.contains("\"changed\":false"));
        host.version.fetch_add(1, Ordering::SeqCst);
        assert!(call("GET", &format!("/api/deck?t={token}&v={ver}")).1.contains("\"changed\":true"));
        // 버튼 누르기: 폴더(연결 프로그램 없음)는 아무 일 없이 성공, 없는 버튼은 오류
        assert!(call("POST", &format!("/api/press?t={token}&id=f")).1.contains("\"ok\":true"));
        assert!(call("POST", &format!("/api/press?t={token}&id=nope")).1.contains("\"ok\":false"));

        // 자동 전환: 바뀐 게 없어도 지금 폴더는 알려줌
        assert!(call("GET", &format!("/api/deck?t={token}&v=999999")).1.contains("\"activeFolder\""));
        // 트랙패드·키보드 조작 (열쇠 필요)
        assert_eq!(call("POST", "/api/control?op=move&dx=5&dy=-3").0, 403);
        assert!(call("POST", &format!("/api/control?t={token}&op=move&dx=5&dy=-3")).1.contains("\"ok\":true"));
        assert!(call("POST", &format!("/api/control?t={token}&op=click&btn=right")).1.contains("\"ok\":true"));
        assert!(call("POST", &format!("/api/control?t={token}&op=key&k=ctrl%2Bc")).1.contains("\"ok\":true"));
        assert_eq!(call("POST", &format!("/api/control?t={token}&op=key&k=%3Cscript%3E")).0, 400, "이상한 키 이름은 거절");
        assert_eq!(call("POST", &format!("/api/control?t={token}&op=nope")).0, 400);
        assert!(call("POST", &format!("/api/control?t={token}&op=media&cmd=next")).1.contains("\"ok\":true"));
        assert!(call("POST", &format!("/api/control?t={token}&op=volume&v=150")).1.contains("\"ok\":true"));
        assert!(call("POST", &format!("/api/control?t={token}&op=mute")).1.contains("\"ok\":true"));
        let got = host.controls.lock().unwrap().clone();
        assert_eq!(
            got,
            ["Move(5, -3)", "Click(\"right\")", "Key(\"ctrl+c\")", "Media(\"next\")", "Volume(100)", "Mute"],
            "볼륨은 100 을 넘지 않게"
        );
        assert_eq!(call("GET", "/api/media").0, 403);
        assert!(call("GET", &format!("/api/media?t={token}")).1.contains("테스트 곡"));
        // 위젯: 열쇠 필요, 타이머는 길게 누르기(처음으로)가 있다고 알려줌
        assert_eq!(call("GET", "/api/widgets").0, 403);
        assert!(call("GET", &format!("/api/widgets?t={token}")).1.contains("\"cpu\":37"));
        let deck: Value = serde_json::from_str(&call("GET", &format!("/api/deck?t={token}")).1).unwrap();
        let kids = deck["buttons"][0]["children"].as_array().unwrap();
        let kinds: Vec<&str> = kids.iter().map(|b| b["kind"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["button", "clock", "timer", "sysinfo", "button"]);
        assert_eq!(kids[2]["minutes"], 5);
        assert_eq!(kids[2]["hasLong"], true);
        assert!(!deck.to_string().contains("\"left\""), "창 배치 방법도 감춤");

        // 휴대폰 화면 스크린샷 (QD_PHONE_SHOT=저장경로 를 주면) — 실제 서버에 붙어서 그린다
        if let Ok(dir) = std::env::var("QD_PHONE_SHOT") {
            let edge = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe";
            for tab in ["grid", "pad", "keys", "music"] {
                let shot = format!("{dir}/phone-{tab}.png");
                let prof = std::env::temp_dir().join(format!("qd-phone-{}-{tab}", std::process::id()));
                let _ = std::process::Command::new(edge)
                    .args([
                        "--headless=new", "--disable-gpu", "--hide-scrollbars",
                        &format!("--user-data-dir={}", prof.display()),
                        "--window-size=500,900", "--force-device-scale-factor=1.5", "--virtual-time-budget=4000",
                        &format!("--screenshot={shot}"),
                        &format!("http://127.0.0.1:{port}/?t={token}&tab={tab}"),
                    ])
                    .output();
                assert!(std::path::Path::new(&shot).exists(), "스크린샷 실패: {tab}");
            }
        }

        server.unblock();
    }

    #[test]
    fn qr_code_renders() {
        let svg = qr_svg("http://192.168.0.2:47823/?t=0123456789abcdef0123456789abcdef");
        assert!(svg.starts_with("<?xml") || svg.contains("<svg"));
    }
}
