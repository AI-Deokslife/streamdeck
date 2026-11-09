//! 화면 가장자리 자동 숨김: 창을 화면 왼쪽·오른쪽·위 끝에 붙여 두면
//! 마우스가 떠날 때 가장자리로 쏙 들어가고, 마우스를 그 끝에 갖다 대면 다시 나온다.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL},
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
        WindowsAndMessaging::{GetCursorPos, GetWindowRect, IsWindowVisible, SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER},
    },
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
    Left,
    Right,
    Top,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl From<RECT> for Rect {
    fn from(r: RECT) -> Self {
        Self { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
    }
}

/// 숨긴 뒤에도 보이게 남겨 두는 두께 (px)
const PEEK: i32 = 4;
/// 이만큼 가까우면 "가장자리에 붙였다"고 본다 (px)
const SNAP: i32 = 16;

/// 창이 붙어 있는 가장자리. 그쪽에 다른 모니터가 이어져 있으면 숨길 수 없으니 제외.
pub fn docked_edge(win: Rect, mon: Rect, work: Rect, neighbor: impl Fn(Edge) -> bool) -> Option<Edge> {
    let near = |a: i32, b: i32, c: i32| (a - b).abs() <= SNAP || (a - c).abs() <= SNAP;
    [
        (Edge::Left, near(win.left, mon.left, work.left)),
        (Edge::Right, near(win.right, mon.right, work.right)),
        (Edge::Top, near(win.top, mon.top, work.top)),
    ]
    .into_iter()
    .find(|&(e, ok)| ok && !neighbor(e))
    .map(|(e, _)| e)
}

/// 숨겼을 때 창의 왼쪽 위 위치
pub fn hidden_pos(win: Rect, mon: Rect, edge: Edge) -> (i32, i32) {
    let (w, h) = (win.right - win.left, win.bottom - win.top);
    match edge {
        Edge::Left => (mon.left - w + PEEK, win.top),
        Edge::Right => (mon.right - PEEK, win.top),
        Edge::Top => (win.left, mon.top - h + PEEK),
    }
}

/// 숨은 창을 다시 꺼낼 위치에 마우스가 왔는지 (화면 끝 + 창이 있던 범위)
pub fn wants_reveal(pt: (i32, i32), win: Rect, mon: Rect, edge: Edge) -> bool {
    let (x, y) = pt;
    match edge {
        Edge::Left => x <= mon.left + PEEK && (win.top..win.bottom).contains(&y),
        Edge::Right => x >= mon.right - 1 - PEEK && (win.top..win.bottom).contains(&y),
        Edge::Top => y <= mon.top + PEEK && (win.left..win.right).contains(&x),
    }
}

/// 창이 작업 영역 안에 얼마나 보이는지 (0.0 ~ 1.0)
pub fn visible_ratio(win: Rect, work: Rect) -> f64 {
    let w = (win.right - win.left).max(1) as f64;
    let h = (win.bottom - win.top).max(1) as f64;
    let iw = (win.right.min(work.right) - win.left.max(work.left)).max(0) as f64;
    let ih = (win.bottom.min(work.bottom) - win.top.max(work.top)).max(0) as f64;
    (iw * ih) / (w * h)
}

/// 창을 작업 영역 안으로 들여놓은 왼쪽 위 위치 (창이 더 크면 왼쪽·위에 맞춘다)
pub fn clamp_into(win: Rect, work: Rect) -> (i32, i32) {
    let (w, h) = (win.right - win.left, win.bottom - win.top);
    let x = win.left.min(work.right - w).max(work.left);
    let y = win.top.min(work.bottom - h).max(work.top);
    (x, y)
}

/// 창이 화면 밖으로 나가 있으면 가장 가까운 모니터 안으로 데려온다.
/// (자동 숨김으로 가장자리에 숨은 채 PC 를 끄면 그 위치가 저장되어, 다음 부팅 때 화면 밖에서 켜지는 문제)
/// 옮겼으면 true.
pub fn ensure_on_screen() -> bool {
    if HIDDEN.lock().unwrap().is_some() {
        return false; // 일부러 숨긴 상태
    }
    let h = hwnd();
    if h.is_null() {
        return false;
    }
    let win = window_rect(h);
    let (_, work) = monitor_rects(unsafe { MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST) });
    // 창 테두리의 보이지 않는 여백(몇 px) 때문에 조금 넘치는 건 그대로 둔다
    if visible_ratio(win, work) >= 0.85 {
        return false;
    }
    let (x, y) = clamp_into(win, work);
    unsafe { SetWindowPos(h, std::ptr::null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) };
    true
}

struct Hidden {
    edge: Edge,
    restore: (i32, i32),
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static PAUSED: AtomicBool = AtomicBool::new(false);
static HIDDEN: Mutex<Option<Hidden>> = Mutex::new(None);

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::SeqCst);
}

/// 편집 중에는 숨기지 않는다 (숨어 있었으면 바로 꺼냄)
pub fn set_paused(on: bool) {
    PAUSED.store(on, Ordering::SeqCst);
    if on {
        reveal();
    }
}

fn hwnd() -> HWND {
    crate::winutil::own_hwnd() as HWND
}

fn monitor_rects(h: windows_sys::Win32::Graphics::Gdi::HMONITOR) -> (Rect, Rect) {
    let mut mi: MONITORINFO = unsafe { std::mem::zeroed() };
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    unsafe { GetMonitorInfoW(h, &mut mi) };
    (mi.rcMonitor.into(), mi.rcWork.into())
}

fn window_rect(h: HWND) -> Rect {
    let mut r: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetWindowRect(h, &mut r) };
    r.into()
}

/// 부드럽게 미끄러지듯 옮긴다
fn slide(h: HWND, from: (i32, i32), to: (i32, i32)) {
    const STEPS: i32 = 7;
    for i in 1..=STEPS {
        // ease-out
        let t = 1.0 - (1.0 - i as f64 / STEPS as f64).powi(3);
        let x = from.0 + ((to.0 - from.0) as f64 * t).round() as i32;
        let y = from.1 + ((to.1 - from.1) as f64 * t).round() as i32;
        unsafe { SetWindowPos(h, std::ptr::null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) };
        thread::sleep(Duration::from_millis(9));
    }
}

/// 숨어 있으면 원래 자리로 꺼낸다. 숨어 있었으면 true.
pub fn reveal() -> bool {
    let Some(hd) = HIDDEN.lock().unwrap().take() else { return false };
    let h = hwnd();
    if !h.is_null() {
        let r = window_rect(h);
        slide(h, (r.left, r.top), hd.restore);
    }
    true
}

pub fn start() {
    thread::spawn(|| {
        let mut away = 0u32;
        loop {
            thread::sleep(Duration::from_millis(100));
            let h = hwnd();
            if h.is_null() {
                continue;
            }
            let active = ENABLED.load(Ordering::SeqCst) && !PAUSED.load(Ordering::SeqCst) && unsafe { IsWindowVisible(h) } != 0;
            if !active {
                reveal();
                away = 0;
                continue;
            }
            let mut p = POINT { x: 0, y: 0 };
            unsafe { GetCursorPos(&mut p) };
            let win = window_rect(h);
            let hidden_edge = HIDDEN.lock().unwrap().as_ref().map(|x| x.edge);
            match hidden_edge {
                Some(edge) => {
                    let (mon, _) = monitor_rects(unsafe { MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST) });
                    if wants_reveal((p.x, p.y), win, mon, edge) {
                        reveal();
                        away = 0;
                    }
                }
                None => {
                    let (mon, work) = monitor_rects(unsafe { MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST) });
                    let neighbor = |e: Edge| {
                        let cy = (win.top + win.bottom) / 2;
                        let cx = (win.left + win.right) / 2;
                        let pt = match e {
                            Edge::Left => POINT { x: mon.left - 8, y: cy },
                            Edge::Right => POINT { x: mon.right + 8, y: cy },
                            Edge::Top => POINT { x: cx, y: mon.top - 8 },
                        };
                        !unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONULL) }.is_null()
                    };
                    let Some(edge) = docked_edge(win, mon, work, neighbor) else {
                        away = 0;
                        continue;
                    };
                    let inside = p.x >= win.left - 2 && p.x < win.right + 2 && p.y >= win.top - 2 && p.y < win.bottom + 2;
                    // 창을 끌고 있는 중(마우스 왼쪽 버튼 누름)에는 숨기지 않는다
                    let dragging = unsafe { GetAsyncKeyState(VK_LBUTTON as i32) } as u16 & 0x8000 != 0;
                    if inside || dragging {
                        away = 0;
                        continue;
                    }
                    away += 1;
                    if away >= 8 {
                        away = 0;
                        let to = hidden_pos(win, mon, edge);
                        *HIDDEN.lock().unwrap() = Some(Hidden { edge, restore: (win.left, win.top) });
                        slide(h, (win.left, win.top), to);
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };

    #[test]
    fn brings_offscreen_window_back() {
        let work = Rect { left: 0, top: 0, right: 1920, bottom: 1040 };
        // 왼쪽 가장자리에 숨은 채 저장된 창 (4px 만 보임)
        let hidden = Rect { left: -416, top: 200, right: 4, bottom: 820 };
        assert!(visible_ratio(hidden, work) < 0.05);
        assert_eq!(clamp_into(hidden, work), (0, 200));
        // 오른쪽·아래로 나간 창
        let out = Rect { left: 1800, top: 900, right: 2220, bottom: 1520 };
        assert_eq!(clamp_into(out, work), (1500, 420));
        // 화면 안에 있거나 테두리 여백만큼 살짝 넘친 창은 그대로
        let ok = Rect { left: -7, top: 100, right: 413, bottom: 720 };
        assert!(visible_ratio(ok, work) >= 0.85);
    }
    const WORK: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1032 };

    #[test]
    fn finds_docked_edge() {
        let none = |_| false;
        let left = Rect { left: 6, top: 200, right: 426, bottom: 530 };
        assert_eq!(docked_edge(left, MON, WORK, none), Some(Edge::Left));
        let right = Rect { left: 1500, top: 200, right: 1915, bottom: 530 };
        assert_eq!(docked_edge(right, MON, WORK, none), Some(Edge::Right));
        let top = Rect { left: 800, top: 0, right: 1220, bottom: 330 };
        assert_eq!(docked_edge(top, MON, WORK, none), Some(Edge::Top));
        let middle = Rect { left: 700, top: 300, right: 1120, bottom: 630 };
        assert_eq!(docked_edge(middle, MON, WORK, none), None);
        // 오른쪽에 모니터가 더 있으면 오른쪽으로는 숨기지 않는다
        assert_eq!(docked_edge(right, MON, WORK, |e| e == Edge::Right), None);
    }

    #[test]
    fn hides_and_reveals() {
        let win = Rect { left: 0, top: 200, right: 420, bottom: 530 };
        assert_eq!(hidden_pos(win, MON, Edge::Left), (-416, 200));
        let hidden = Rect { left: -416, top: 200, right: 4, bottom: 530 };
        assert!(wants_reveal((0, 300), hidden, MON, Edge::Left));
        assert!(!wants_reveal((0, 900), hidden, MON, Edge::Left), "창이 없던 높이에서는 안 나옴");
        assert!(!wants_reveal((50, 300), hidden, MON, Edge::Left));
        let win = Rect { left: 1500, top: 200, right: 1920, bottom: 530 };
        assert_eq!(hidden_pos(win, MON, Edge::Right), (1916, 200));
        assert!(wants_reveal((1919, 250), Rect { left: 1916, top: 200, right: 2336, bottom: 530 }, MON, Edge::Right));
        let win = Rect { left: 800, top: 0, right: 1220, bottom: 330 };
        assert_eq!(hidden_pos(win, MON, Edge::Top), (800, -326));
    }
}
