//! 정보를 보여주는 버튼(위젯): 타이머, CPU·메모리 사용률. (시계는 화면에서 바로 그린다)
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use windows_sys::Win32::{
    Foundation::FILETIME,
    System::{
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
        Threading::GetSystemTimes,
    },
};

// ---------------- 타이머 ----------------

#[derive(Clone)]
struct Timer {
    total: Duration,
    /// 돌아가는 중이면 끝나는 시각
    ends_at: Option<Instant>,
    /// 멈춰 있을 때 남은 시간
    remaining: Duration,
    /// 다 됐음 (다음에 누르면 처음부터)
    done: bool,
}

static TIMERS: Mutex<Option<HashMap<String, Timer>>> = Mutex::new(None);

fn with_timers<R>(f: impl FnOnce(&mut HashMap<String, Timer>) -> R) -> R {
    let mut g = TIMERS.lock().unwrap();
    f(g.get_or_insert_with(HashMap::new))
}

/// 누르기: 멈춰 있으면 시작, 돌아가면 일시정지, 끝났으면 처음부터 다시
pub fn timer_press(id: &str, minutes: u32) {
    let total = Duration::from_secs(u64::from(minutes.clamp(1, 24 * 60)) * 60);
    with_timers(|t| {
        let e = t.entry(id.to_string()).or_insert(Timer { total, ends_at: None, remaining: total, done: false });
        if e.total != total && e.ends_at.is_none() {
            // 편집에서 시간을 바꿨으면 새 시간으로
            *e = Timer { total, ends_at: None, remaining: total, done: false };
        }
        match e.ends_at {
            Some(end) => {
                e.remaining = end.saturating_duration_since(Instant::now());
                e.ends_at = None;
            }
            None => {
                if e.done || e.remaining.is_zero() {
                    e.remaining = e.total;
                    e.done = false;
                }
                e.ends_at = Some(Instant::now() + e.remaining);
            }
        }
    });
}

/// 처음 시간으로 되돌리기 (길게 누르기)
pub fn timer_reset(id: &str) {
    with_timers(|t| {
        if let Some(e) = t.get_mut(id) {
            *e = Timer { total: e.total, ends_at: None, remaining: e.total, done: false };
        }
    });
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TimerView {
    pub remaining_ms: u64,
    pub running: bool,
    pub done: bool,
}

fn timer_views() -> HashMap<String, TimerView> {
    with_timers(|t| {
        t.iter()
            .map(|(id, e)| {
                let remaining = e.ends_at.map(|end| end.saturating_duration_since(Instant::now())).unwrap_or(e.remaining);
                (id.clone(), TimerView { remaining_ms: remaining.as_millis() as u64, running: e.ends_at.is_some(), done: e.done })
            })
            .collect()
    })
}

/// 끝난 타이머를 찾아 표시하고 알린다 (소리 + 화면 알림)
pub fn start_watcher(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(400));
        let finished: Vec<String> = with_timers(|t| {
            let now = Instant::now();
            t.iter_mut()
                .filter(|(_, e)| e.ends_at.is_some_and(|end| end <= now))
                .map(|(id, e)| {
                    e.ends_at = None;
                    e.remaining = Duration::ZERO;
                    e.done = true;
                    id.clone()
                })
                .collect()
        });
        for id in finished {
            unsafe { windows_sys::Win32::System::Diagnostics::Debug::MessageBeep(0x40) };
            let _ = app.emit("timer-done", id);
            crate::remote::bump_version(&app);
        }
    });
}

// ---------------- CPU · 메모리 ----------------

static LAST_CPU: Mutex<Option<(u64, u64)>> = Mutex::new(None);

fn ft(f: FILETIME) -> u64 {
    (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)
}

/// 지난번 호출 이후의 CPU 사용률 (처음엔 0)
fn cpu_percent() -> u32 {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return 0;
    }
    let (idle, total) = (ft(idle), ft(kernel) + ft(user)); // kernel 시간에 idle 이 포함돼 있다
    let mut last = LAST_CPU.lock().unwrap();
    let pct = match *last {
        Some((li, lt)) if total > lt => {
            let busy = (total - lt).saturating_sub(idle - li);
            ((busy as f64 / (total - lt) as f64) * 100.0).round() as u32
        }
        _ => 0,
    };
    *last = Some((idle, total));
    pct.min(100)
}

fn memory_percent() -> u32 {
    let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut m) } == 0 {
        return 0;
    }
    m.dwMemoryLoad
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetState {
    pub cpu: u32,
    pub memory: u32,
    pub timers: HashMap<String, TimerView>,
}

pub fn snapshot() -> WidgetState {
    WidgetState { cpu: cpu_percent(), memory: memory_percent(), timers: timer_views() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_start_pause_reset() {
        let id = "test-timer-1";
        timer_press(id, 5);
        let v = &timer_views()[id];
        assert!(v.running && v.remaining_ms > 299_000 && v.remaining_ms <= 300_000);
        thread::sleep(Duration::from_millis(30));
        timer_press(id, 5); // 일시정지
        let paused = timer_views()[id].clone();
        assert!(!paused.running && paused.remaining_ms < 300_000);
        thread::sleep(Duration::from_millis(30));
        assert_eq!(timer_views()[id].remaining_ms, paused.remaining_ms, "멈춘 동안 줄면 안 됨");
        timer_reset(id);
        assert_eq!(timer_views()[id].remaining_ms, 300_000);
    }

    #[test]
    fn system_usage_in_range() {
        let _ = cpu_percent();
        thread::sleep(Duration::from_millis(100));
        assert!(cpu_percent() <= 100);
        let m = memory_percent();
        assert!(m > 0 && m <= 100, "메모리 사용률 {m}");
    }
}
