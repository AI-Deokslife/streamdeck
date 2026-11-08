//! 자동 업데이트: GitHub Releases 의 latest.json 을 보고 새 버전이 있으면 알려 주고,
//! 사용자가 누르면 내려받아 설치한다. (설치 파일은 서명이 맞을 때만 설치된다)
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: String,
}

async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    Ok(update.map(|u| UpdateInfo { version: u.version.clone(), current: u.current_version.clone(), notes: u.body.clone().unwrap_or_default() }))
}

/// 새 버전 확인. manual=true(트레이 메뉴)면 "최신이에요"·오류도 알려 준다.
pub fn check_in_background(app: AppHandle, manual: bool, delay: Duration) {
    tauri::async_runtime::spawn(async move {
        tokio_sleep(delay).await;
        match check(&app).await {
            Ok(Some(info)) => {
                let _ = app.emit("update-available", info);
            }
            Ok(None) => {
                if manual {
                    let v = app.package_info().version.to_string();
                    status(&app, format!("지금 최신 버전이에요 (v{v})"), "info");
                }
            }
            Err(e) => {
                eprintln!("업데이트 확인 실패: {e}");
                if manual {
                    status(&app, friendly_error(&e), "error");
                }
            }
        }
    });
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
    use super::friendly_error;

    #[test]
    fn friendly_messages() {
        assert!(friendly_error("Could not fetch a valid release JSON from the remote").contains("아직 GitHub"));
        assert!(friendly_error("io: failed to lookup address (dns error)").contains("인터넷"));
        assert!(friendly_error("something else").contains("something else"));
    }
}
