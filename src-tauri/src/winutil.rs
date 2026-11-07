//! Windows 전용 처리: 포커스 뺏지 않는 창, 이전 활성 창 추적, 수정키 해제 대기.
use std::{
    sync::atomic::{AtomicIsize, Ordering},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT},
        WindowsAndMessaging::{
            GetForegroundWindow, GetWindowLongPtrW, SetForegroundWindow, SetWindowLongPtrW,
            GWL_EXSTYLE, WS_EX_NOACTIVATE,
        },
    },
};

/// 우리 창의 HWND
static OWN_HWND: AtomicIsize = AtomicIsize::new(0);
/// 우리 창이 아닌, 마지막으로 활성화됐던 창
static LAST_FOREIGN: AtomicIsize = AtomicIsize::new(0);

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

/// 백그라운드에서 활성 창을 계속 기록한다.
pub fn start_foreground_tracker() {
    thread::spawn(|| loop {
        let fg = unsafe { GetForegroundWindow() } as isize;
        if fg != 0 && fg != OWN_HWND.load(Ordering::SeqCst) {
            LAST_FOREIGN.store(fg, Ordering::SeqCst);
        }
        thread::sleep(Duration::from_millis(150));
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
