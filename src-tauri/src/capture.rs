//! 화면 일부를 캡처해서 버튼 이미지로 쓰기.
//! Windows 캡처 도구(ms-screenclip: = Win+Shift+S 와 같은 화면)를 띄우고,
//! 사용자가 영역을 고르면 클립보드에 들어온 이미지를 PNG 로 꺼낸다.
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use crate::winutil;
use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;

/// 사용자가 캡처를 기다리다 취소했는지
static CANCEL: AtomicBool = AtomicBool::new(false);
/// 동시에 두 번 캡처하지 않게
static RUNNING: AtomicBool = AtomicBool::new(false);
/// 영역을 고를 때까지 기다리는 최대 시간
const TIMEOUT: Duration = Duration::from_secs(60);

pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

/// 캡처 도구를 띄우고, 캡처된 이미지를 PNG 바이트로 돌려준다.
/// 캡처하는 동안 우리 창은 숨겨서 화면을 가리지 않게 한다.
pub fn capture_region(app: &AppHandle) -> Result<Vec<u8>, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("이미 캡처 중이에요".into());
    }
    CANCEL.store(false, Ordering::SeqCst);
    let window = app.get_webview_window("main");
    if let Some(w) = &window {
        let _ = w.hide();
    }
    thread::sleep(Duration::from_millis(250)); // 창이 사라진 뒤에 캡처 화면이 뜨도록

    let result = wait_for_capture(app);

    if let Some(w) = &window {
        let _ = w.show();
        let _ = w.set_focus();
    }
    RUNNING.store(false, Ordering::SeqCst);
    result
}

fn wait_for_capture(app: &AppHandle) -> Result<Vec<u8>, String> {
    let mut seq = unsafe { GetClipboardSequenceNumber() };
    app.opener()
        .open_url("ms-screenclip:", None::<&str>)
        .map_err(|e| format!("캡처 도구를 열지 못했어요: {e}"))?;

    let start = Instant::now();
    // 캡처 화면(오버레이) 창. Esc 로 닫으면 클립보드가 안 바뀌므로, 창이 사라진 걸 보고 취소로 판단한다
    let mut overlay: Option<isize> = None;
    let mut closed_at: Option<Instant> = None;
    while start.elapsed() < TIMEOUT {
        if CANCEL.load(Ordering::SeqCst) {
            return Err("캡처를 취소했어요".into());
        }
        let now = unsafe { GetClipboardSequenceNumber() };
        if now != seq {
            seq = now;
            // 클립보드가 바뀌었는데 이미지가 아니면(글자를 복사한 경우 등) 계속 기다린다
            if let Some(png) = read_clipboard_png() {
                return Ok(png);
            }
        }

        match (overlay, winutil::foreground_app()) {
            (None, Some((hwnd, exe))) if is_snip_overlay(&exe) => overlay = Some(hwnd),
            (Some(o), fg) => {
                let still_open = winutil::is_window_visible(o) && fg.is_some_and(|(h, _)| h == o);
                if still_open {
                    closed_at = None;
                } else if closed_at.is_none() {
                    closed_at = Some(Instant::now());
                }
            }
            _ => {}
        }
        // 오버레이가 닫히고 잠시 기다려도 이미지가 안 들어오면 사용자가 취소한 것
        if closed_at.is_some_and(|t| t.elapsed() > CLOSE_GRACE) {
            return Err("캡처를 취소했어요".into());
        }
        thread::sleep(Duration::from_millis(150));
    }
    Err("시간이 지나 캡처를 멈췄어요. 다시 눌러 주세요".into())
}

/// 오버레이가 닫힌 뒤 클립보드에 이미지가 들어올 때까지 기다려 주는 시간
const CLOSE_GRACE: Duration = Duration::from_millis(2500);

/// Windows 캡처 화면을 띄우는 프로세스 (Windows 10/11 버전에 따라 다름)
fn is_snip_overlay(exe: &str) -> bool {
    ["ScreenClippingHost.exe", "SnippingTool.exe", "ScreenSketch.exe"]
        .iter()
        .any(|n| n.eq_ignore_ascii_case(exe))
}

/// 클립보드의 이미지를 PNG 로. 캡처 도구가 막 쓰는 중이면 잠깐 기다렸다 다시 시도
fn read_clipboard_png() -> Option<Vec<u8>> {
    for _ in 0..5 {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            if let Ok(img) = cb.get_image() {
                return crate::icon::encode_png(img.width as u32, img.height as u32, &img.bytes).ok();
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    None
}

#[cfg(test)]
mod tests {
    /// 클립보드 이미지 → PNG 변환 경로 확인 (사용자 클립보드를 덮어쓰므로 수동 실행)
    /// cargo test --lib clipboard_image_roundtrip -- --ignored
    #[test]
    #[ignore]
    fn clipboard_image_roundtrip() {
        let mut cb = arboard::Clipboard::new().unwrap();
        let (w, h) = (40usize, 20usize);
        let bytes: Vec<u8> = (0..w * h).flat_map(|i| [(i % 255) as u8, 80, 160, 255]).collect();
        cb.set_image(arboard::ImageData { width: w, height: h, bytes: bytes.into() }).unwrap();
        let png = super::read_clipboard_png().expect("클립보드 이미지 못 읽음");
        assert_eq!(&png[1..4], b"PNG");
    }
}
