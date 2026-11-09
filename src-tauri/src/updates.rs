//! 자동 업데이트: GitHub Releases 의 latest.json 을 보고 새 버전이 있으면 알려 주고,
//! 사용자가 누르면 내려받아 설치한다. (설치 파일은 서명이 맞을 때만 설치된다)
use serde::Serialize;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: String,
    /// 알림 줄에 보여 줄 한 줄 요약
    pub headline: String,
}

async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    Ok(update.map(|u| {
        let notes = u.body.clone().unwrap_or_default();
        UpdateInfo { version: u.version.clone(), current: u.current_version.clone(), headline: headline(&notes), notes }
    }))
}

/// 켠 뒤 처음 확인까지 기다리는 시간
const FIRST_CHECK: Duration = Duration::from_secs(8);
/// 켜 둔 채로 며칠씩 쓰는 PC 를 위해 이 간격마다 다시 확인
const RECHECK: Duration = Duration::from_secs(4 * 60 * 60);
/// '나중에'를 누르면 이 시간 동안은 같은 버전으로 다시 띄우지 않는다
const SNOOZE: Duration = Duration::from_secs(24 * 60 * 60);

/// '나중에'를 누른 버전과 그 시각
static SNOOZED: Mutex<Option<(String, Instant)>> = Mutex::new(None);

fn snoozed(version: &str) -> bool {
    matches!(&*SNOOZED.lock().unwrap(), Some((v, at)) if v == version && at.elapsed() < SNOOZE)
}

/// 업데이트 알림 줄의 '나중에'(✕)
#[tauri::command]
pub fn snooze_update(version: String) {
    *SNOOZED.lock().unwrap() = Some((version, Instant::now()));
}

/// 앱을 켜 두는 동안 계속: 켠 직후 한 번, 그 뒤로 몇 시간마다 새 버전을 확인한다
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio_sleep(FIRST_CHECK).await;
        loop {
            run_check(&app, false).await;
            tokio_sleep(RECHECK).await;
        }
    });
}

/// 새 버전 확인. manual=true(트레이 메뉴)면 "최신이에요"·오류도 알려 준다.
pub fn check_in_background(app: AppHandle, manual: bool, delay: Duration) {
    tauri::async_runtime::spawn(async move {
        tokio_sleep(delay).await;
        run_check(&app, manual).await;
    });
}

async fn run_check(app: &AppHandle, manual: bool) {
    let app = app.clone();
    {
        match check(&app).await {
            Ok(Some(info)) => {
                // 새 버전이 있으면 창을 띄워 알림 줄이 바로 보이게 (트레이·가장자리에 숨어 있어도)
                if manual || !snoozed(&info.version) {
                    crate::show_window(&app);
                    let _ = app.emit("update-available", info);
                }
            }
            Ok(None) => {
                if manual {
                    let v = app.package_info().version.to_string();
                    crate::show_window(&app);
                    status(&app, format!("지금 최신 버전이에요 (v{v})"), "info");
                }
            }
            Err(e) => {
                eprintln!("업데이트 확인 실패: {e}");
                if manual {
                    crate::show_window(&app);
                    status(&app, friendly_error(&e), "error");
                }
            }
        }
    }
}

/// 릴리스 설명에서 알림 줄에 보여 줄 한 줄 요약 (첫 줄, 앞의 '- ' 는 뗀다)
pub fn headline(notes: &str) -> String {
    notes
        .lines()
        .map(|l| l.trim().trim_start_matches(['-', '*', '•']).trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .chars()
        .take(60)
        .collect()
}

async fn tokio_sleep(d: Duration) {
    if !d.is_zero() {
        let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
    }
}

/// 트레이 '업데이트 확인' 결과 알림 (kind: info / error)
#[derive(Serialize, Clone)]
struct Status {
    text: String,
    kind: &'static str,
}

fn status(app: &AppHandle, text: String, kind: &'static str) {
    let _ = app.emit("update-status", Status { text, kind });
}

/// 업데이트 확인 오류를 알기 쉬운 말로
fn friendly_error(e: &str) -> String {
    let low = e.to_lowercase();
    if low.contains("release json") || low.contains("404") || low.contains("not found") {
        "아직 GitHub에 올라온 업데이트 정보가 없어요. 지금 버전을 그대로 쓰시면 돼요".into()
    } else if low.contains("dns") || low.contains("connect") || low.contains("timed out") || low.contains("network") {
        "인터넷에 연결할 수 없어서 업데이트를 확인하지 못했어요".into()
    } else {
        format!("업데이트를 확인하지 못했어요 ({e})")
    }
}

#[derive(Serialize, Clone)]
struct Progress {
    got: u64,
    total: Option<u64>,
}

/// 내려받아 설치하고 다시 시작
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("새 버전이 없어요")?;
    let mut got = 0u64;
    let progress_app = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                got += chunk as u64;
                let _ = progress_app.emit("update-progress", Progress { got, total });
            },
            || {},
        )
        .await
        .map_err(|e| format!("업데이트를 설치하지 못했어요: {e}"))?;
    // 숨은 자리 말고 원래 자리를 기억하도록
    crate::autohide::reveal();
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::{friendly_error, headline};

    #[test]
    fn headline_is_first_line() {
        assert_eq!(headline("휴대폰 리모컨 버튼 크기 조절\n- 자세히"), "휴대폰 리모컨 버튼 크기 조절");
        assert_eq!(headline("\n- 첫 항목\n- 둘째"), "첫 항목");
        assert_eq!(headline(""), "");
    }

    #[test]
    fn friendly_messages() {
        assert!(friendly_error("Could not fetch a valid release JSON from the remote").contains("아직 GitHub"));
        assert!(friendly_error("io: failed to lookup address (dns error)").contains("인터넷"));
        assert!(friendly_error("something else").contains("something else"));
    }
}
