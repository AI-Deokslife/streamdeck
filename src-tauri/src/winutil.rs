//! Windows 전용 처리: 포커스 뺏지 않는 창, 활성 프로그램 추적, 특정 프로그램 창 활성화, 수정키 해제 대기.
use serde::Serialize;
use std::{
    sync::atomic::{AtomicIsize, Ordering},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HWND, LPARAM},
    System::Threading::{
        AttachThreadInput, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_QUERY_LIMITED_INFORMATION,
    },
    UI::{
        Input::KeyboardAndMouse::{
            keybd_event, GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_MOUSE, KEYEVENTF_KEYUP, MOUSEEVENTF_MOVE,
            MOUSEINPUT, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
        },
        WindowsAndMessaging::{
            BringWindowToTop, EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongPtrW,
            GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
            SetForegroundWindow, SetWindowLongPtrW, ShowWindow, GWL_EXSTYLE, GW_OWNER, SW_RESTORE,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        },
    },
};

/// 우리 창의 HWND
static OWN_HWND: AtomicIsize = AtomicIsize::new(0);
/// 우리 창이 아닌, 마지막으로 활성화됐던 창
static LAST_FOREIGN: AtomicIsize = AtomicIsize::new(0);
/// 마지막으로 앞에 있던 (우리 창이 아닌) 프로그램 이름
static LAST_EXE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

pub fn last_app() -> String {
    LAST_EXE.lock().unwrap().clone()
}

pub fn set_own_hwnd(hwnd: isize) {
    OWN_HWND.store(hwnd, Ordering::SeqCst);
}

/// 창 클릭 시 활성화되지 않도록(WS_EX_NOACTIVATE) 설정/해제.
/// 편집 모드에서는 글자 입력을 위해 해제해야 한다.
pub fn set_no_activate(enable: bool) {
    let hwnd = OWN_HWND.load(Ordering::SeqCst) as HWND;
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let flag = WS_EX_NOACTIVATE as isize;
        let new_style = if enable { style | flag } else { style & !flag };
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);
    }
}

// ---------------- 프로그램 정보 ----------------

/// 창을 만든 프로세스의 실행 파일 전체 경로
fn exe_path_of(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(handle);
        (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// "C:\...\Hwp.exe" → "Hwp.exe"
pub fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

fn window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}

/// 작업 표시줄에 보일 만한 일반 창인지 (숨김/도구 창/소유 창 제외)
fn is_app_window(hwnd: HWND) -> bool {
    unsafe {
        if IsWindowVisible(hwnd) == 0 || !GetWindow(hwnd, GW_OWNER).is_null() {
            return false;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        ex & WS_EX_TOOLWINDOW == 0 && GetWindowTextLengthW(hwnd) > 0
    }
}

fn all_app_windows() -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> i32 {
        let list = unsafe { &mut *(lparam as *mut Vec<HWND>) };
        if is_app_window(hwnd) {
            list.push(hwnd);
        }
        1
    }
    let mut list: Vec<HWND> = Vec::new();
    unsafe { EnumWindows(Some(collect), &mut list as *mut _ as LPARAM) };
    list
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RunningApp {
    /// 실행 파일 이름 (예: Hwp.exe)
    pub exe: String,
    /// 실행 파일 전체 경로
    pub path: String,
    /// 대표 창 제목
    pub title: String,
}

/// 지금 창이 열려 있는 프로그램 목록 (프로그램당 하나)
pub fn running_apps() -> Vec<RunningApp> {
    let own = OWN_HWND.load(Ordering::SeqCst);
    let mut out: Vec<RunningApp> = Vec::new();
    for hwnd in all_app_windows() {
        if hwnd as isize == own {
            continue;
        }
        let Some(path) = exe_path_of(hwnd) else { continue };
        let exe = file_name(&path).to_string();
        if out.iter().any(|a| a.exe.eq_ignore_ascii_case(&exe)) {
            continue;
        }
        out.push(RunningApp { exe, path, title: window_title(hwnd) });
    }
    out.sort_by_key(|a| a.exe.to_lowercase());
    out
}

/// 지금 맨 앞 창의 (HWND, 실행 파일 이름)
pub fn foreground_app() -> Option<(isize, String)> {
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_null() {
        return None;
    }
    exe_path_of(fg).map(|p| (fg as isize, file_name(&p).to_string()))
}

/// 창이 아직 살아 있고 보이는지
pub fn is_window_visible(hwnd: isize) -> bool {
    unsafe { IsWindowVisible(hwnd as HWND) != 0 }
}

// ---------------- 활성 창 추적 ----------------

/// 백그라운드에서 활성 창을 계속 기록하고, 활성 프로그램이 바뀌면 `on_app_change(exe 이름)`을 부른다.
/// 우리 창이 앞에 올 때는 무시한다.
pub fn start_foreground_tracker(on_app_change: impl Fn(String) + Send + 'static) {
    thread::spawn(move || {
        let mut last_hwnd = 0isize;
        let mut last_exe = String::new();
        loop {
            let fg = unsafe { GetForegroundWindow() } as isize;
            if fg != 0 && fg != OWN_HWND.load(Ordering::SeqCst) {
                LAST_FOREIGN.store(fg, Ordering::SeqCst);
                if fg != last_hwnd {
                    last_hwnd = fg;
                    if let Some(path) = exe_path_of(fg as HWND) {
                        let exe = file_name(&path).to_string();
                        if exe != last_exe {
                            last_exe = exe.clone();
                            *LAST_EXE.lock().unwrap() = exe.clone();
                            on_app_change(exe);
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(200));
        }
    });
}

/// 키 입력을 보내기 전, 우리 창이 활성 상태라면 직전 창으로 포커스를 돌려준다.
pub fn restore_foreign_focus() {
    let own = OWN_HWND.load(Ordering::SeqCst);
    let fg = unsafe { GetForegroundWindow() } as isize;
    let last = LAST_FOREIGN.load(Ordering::SeqCst);
    if fg == own && last != 0 {
        unsafe { SetForegroundWindow(last as HWND) };
        thread::sleep(Duration::from_millis(80));
    }
}

// ---------------- 특정 프로그램 창 활성화 ----------------

#[cfg(test)]
fn exe_matches(hwnd: HWND, exe: &str) -> bool {
    exe_matches_any(hwnd, &[exe])
}

/// 창이 exes 가운데 하나의 프로그램 것인지
fn exe_matches_any<S: AsRef<str>>(hwnd: HWND, exes: &[S]) -> bool {
    exe_path_of(hwnd).is_some_and(|p| {
        let name = file_name(&p);
        exes.iter().any(|e| name.eq_ignore_ascii_case(e.as_ref().trim()))
    })
}

/// 창이 실제로 앞에 올 때까지 잠깐 기다린다 (활성화는 비동기라 바로 확인하면 실패로 보일 수 있음)
fn wait_foreground<S: AsRef<str>>(hwnd: HWND, exes: &[S], within: Duration) -> bool {
    let start = Instant::now();
    loop {
        let fg = unsafe { GetForegroundWindow() };
        if fg == hwnd || (!fg.is_null() && exe_matches_any(fg, exes)) {
            return true;
        }
        if start.elapsed() >= within {
            return false;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// 아무 효과 없는 마우스 입력(0만큼 이동)을 보낸다.
/// Windows는 "마지막 입력을 받은 프로그램"에만 창 전환을 허락하는데, QuickDeck 버튼 클릭은
/// 화면을 그리는 WebView2 프로세스가 받으므로 이 입력으로 권한을 얻는다.
fn send_null_input() {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 0, dwFlags: MOUSEEVENTF_MOVE, time: 0, dwExtraInfo: 0 },
        },
    };
    unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
}

/// 창을 맨 앞으로 가져온다. Windows의 창 전환 제한에 걸리면 단계별로 다른 방법을 시도한다.
fn bring_to_front<S: AsRef<str>>(hwnd: HWND, exes: &[S]) -> bool {
    let quick = Duration::from_millis(250);
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        // 1) 그냥 요청
        SetForegroundWindow(hwnd);
        if wait_foreground(hwnd, exes, quick) {
            return true;
        }
        eprintln!("창 전환: 기본 요청이 거절됨 → 우회 방법 시도");
        // 2) 빈 입력으로 "마지막 입력" 권한을 얻은 뒤 다시
        send_null_input();
        SetForegroundWindow(hwnd);
        if wait_foreground(hwnd, exes, quick) {
            return true;
        }
        // 3) Alt 를 두 번 눌렀다 떼면 창 전환 잠금이 풀린다 (두 번이라 메뉴는 열리지 않음)
        for _ in 0..2 {
            keybd_event(VK_MENU as u8, 0, 0, 0);
            keybd_event(VK_MENU as u8, 0, KEYEVENTF_KEYUP, 0);
        }
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        if wait_foreground(hwnd, exes, quick) {
            return true;
        }
        // 4) 지금 앞에 있는 창의 입력 스레드에 잠깐 붙어서 시도
        let fg = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg, std::ptr::null_mut());
        let target_thread = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());
        if fg_thread != 0 && target_thread != 0 && fg_thread != target_thread {
            AttachThreadInput(target_thread, fg_thread, 1);
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
            AttachThreadInput(target_thread, fg_thread, 0);
        }
        wait_foreground(hwnd, exes, Duration::from_millis(400))
    }
}

pub enum FocusError {
    NotRunning,
    Failed,
}

/// `exes` 가운데 실행 중인 프로그램의 창을 앞으로 가져온다 (예: 크롬·엣지·웨일 중 열려 있는 것).
/// 이미 그중 하나가 앞에 있으면 아무것도 안 한다. 여러 개가 열려 있으면 가장 최근에 쓴 창.
/// 하나도 실행 중이 아니면 Err(NotRunning).
pub fn focus_app<S: AsRef<str>>(exes: &[S]) -> Result<(), FocusError> {
    let fg = unsafe { GetForegroundWindow() };
    if !fg.is_null() && exe_matches_any(fg, exes) {
        return Ok(());
    }
    // 마지막으로 쓰던 창을 우선, 없으면 화면 위쪽(=최근에 쓴) 순서로 첫 번째 창
    let last = LAST_FOREIGN.load(Ordering::SeqCst) as HWND;
    let target = if !last.is_null() && exe_matches_any(last, exes) {
        Some(last)
    } else {
        all_app_windows().into_iter().find(|&h| exe_matches_any(h, exes))
    };
    let Some(hwnd) = target else {
        return Err(FocusError::NotRunning);
    };
    if bring_to_front(hwnd, exes) {
        thread::sleep(Duration::from_millis(120)); // 창이 키 입력을 받을 준비 시간
        Ok(())
    } else {
        Err(FocusError::Failed)
    }
}

/// 전역 단축키로 실행된 경우 사용자가 아직 Ctrl/Alt 등을 누르고 있으면
/// 보낼 키와 섞이므로, 손을 뗄 때까지(최대 1초) 기다린다.
pub fn wait_modifiers_released() {
    let start = Instant::now();
    let keys = [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN];
    while start.elapsed() < Duration::from_secs(1) {
        let any_down = keys
            .iter()
            .any(|&k| unsafe { GetAsyncKeyState(k as i32) } as u16 & 0x8000 != 0);
        if !any_down {
            return;
        }
        thread::sleep(Duration::from_millis(15));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 여러 프로그램 중 하나만 열려 있어도 찾아야 한다 (크롬은 없고 웨일만 열린 경우 등).
    /// 지금 앞에 있는 창을 그대로 쓰므로 화면이 바뀌지 않는다.
    #[test]
    fn any_of_several_apps_matches() {
        let fg = unsafe { GetForegroundWindow() };
        let Some(path) = (!fg.is_null()).then(|| exe_path_of(fg)).flatten() else {
            return; // 데스크톱이 없는 환경 (CI 등)
        };
        let exe = file_name(&path).to_string();
        let list = ["no_such_browser.exe".to_string(), exe.to_uppercase()];
        assert!(exe_matches_any(fg, &list));
        assert!(matches!(focus_app(&list), Ok(())), "앞에 있는 {exe}를 못 찾음");
        assert!(matches!(focus_app(&["no_such_a.exe", "no_such_b.exe"]), Err(FocusError::NotRunning)));
    }

    /// 실제 데스크톱이 필요하므로 수동 실행: cargo test --lib -- --ignored
    #[test]
    #[ignore]
    fn focus_notepad_end_to_end() {
        let mut child = std::process::Command::new("notepad.exe").spawn().unwrap();
        thread::sleep(Duration::from_millis(1500));
        let apps = running_apps();
        assert!(apps.iter().any(|a| a.exe.eq_ignore_ascii_case("notepad.exe")), "목록에 메모장 없음");
        assert!(matches!(focus_app(&["notepad.exe"]), Ok(())), "메모장 활성화 실패");
        let fg = unsafe { GetForegroundWindow() };
        assert!(exe_matches(fg, "notepad.exe"), "앞에 있는 창이 메모장이 아님");
        assert!(matches!(focus_app(&["no_such_app_123.exe"]), Err(FocusError::NotRunning)));
        let _ = child.kill();
    }
}

#[cfg(test)]
mod focus_real {
    use super::*;

    /// 백그라운드 프로그램에서 실제로 열려 있는 브라우저 창을 앞으로 가져오는지 (창 전환 제한 상황 재현).
    /// 끝나면 원래 앞에 있던 창으로 되돌린다. 수동 실행: FOCUS_EXE=chrome.exe cargo test --lib bring_browser -- --ignored
    #[test]
    #[ignore]
    fn bring_browser_to_front_from_background() {
        let exe = std::env::var("FOCUS_EXE").unwrap_or_else(|_| "chrome.exe".into());
        let before = unsafe { GetForegroundWindow() };
        let result = focus_app(&[exe.as_str()]);
        let now = foreground_app().map(|(_, e)| e).unwrap_or_default();
        // 원래 창으로 복귀
        if !before.is_null() {
            bring_to_front(before, &[] as &[&str]);
        }
        assert!(matches!(result, Ok(())), "{exe} 창을 앞으로 가져오지 못함");
        assert!(now.eq_ignore_ascii_case(&exe), "앞에 온 창: {now}");
    }
}
