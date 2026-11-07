//! 실행 파일 이름(예: Hwp.exe)으로 설치 경로 찾기.
//! 1) 레지스트리 App Paths  2) PATH  3) Program Files 등 설치 폴더 탐색 (결과는 캐시)
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::Mutex,
};
use windows_sys::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
};

static CACHE: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

/// 설치 폴더 탐색 깊이 (예: Program Files (x86)\Hnc\Office 2020\HOffice110\Bin\Hwp.exe = 4단계)
const MAX_DEPTH: usize = 5;
/// 너무 오래 걸리지 않게 살펴볼 폴더 수 상한
const MAX_DIRS: usize = 40_000;

pub fn resolve(exe: &str) -> Option<String> {
    let exe = exe.trim();
    if exe.is_empty() {
        return None;
    }
    let key = exe.to_lowercase();
    if let Some(hit) = CACHE.lock().unwrap().as_ref().and_then(|c| c.get(&key)) {
        return hit.clone();
    }
    let found = from_app_paths(exe).or_else(|| from_path_env(exe)).or_else(|| scan_install_dirs(exe));
    // 찾은 경우만 캐시 (못 찾았으면 설치 후 다시 시도할 수 있게)
    if found.is_some() {
        CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, found.clone());
    }
    found
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn reg_default_value(root: HKEY, subkey: &str) -> Option<String> {
    let sub = wide(subkey);
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(
            root,
            sub.as_ptr(),
            std::ptr::null(),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if rc != 0 {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    let raw = String::from_utf16_lossy(&buf[..len]);
    let path = raw.trim().trim_matches('"').to_string();
    Path::new(&path).is_file().then_some(path)
}

fn from_app_paths(exe: &str) -> Option<String> {
    let subs = [
        format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{exe}"),
        format!(r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths\{exe}"),
    ];
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .flat_map(|root| subs.iter().map(move |s| (root, s)))
        .find_map(|(root, s)| reg_default_value(root, s))
}

fn from_path_env(exe: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(exe))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

fn scan_install_dirs(exe: &str) -> Option<String> {
    let mut roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
        .iter()
        .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
        .collect();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Programs"));
    }
    roots.dedup();

    let mut matches: Vec<PathBuf> = Vec::new();
    let mut queue: VecDeque<(PathBuf, usize)> = roots.into_iter().map(|r| (r, 0)).collect();
    let mut visited = 0usize;
    while let Some((dir, depth)) = queue.pop_front() {
        visited += 1;
        if visited > MAX_DIRS {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            let name = entry.file_name();
            if ft.is_file() && name.to_string_lossy().eq_ignore_ascii_case(exe) {
                matches.push(entry.path());
            } else if ft.is_dir() && !ft.is_symlink() && depth < MAX_DEPTH {
                let n = name.to_string_lossy();
                // 시간만 잡아먹는 시스템 폴더는 건너뛴다
                if !matches!(n.as_ref(), "WindowsApps" | "Windows Defender" | "Windows Kits" | "dotnet" | "Reference Assemblies") {
                    queue.push_back((entry.path(), depth + 1));
                }
            }
        }
    }
    // 여러 버전이 깔려 있으면 경로상 가장 최신으로 보이는 것 (예: HOffice120 > HOffice110)
    matches.into_iter().max().map(|p| p.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_system_programs() {
        // App Paths 또는 PATH 로 항상 찾을 수 있는 프로그램
        let p = resolve("notepad.exe").expect("notepad.exe 경로를 못 찾음");
        assert!(p.to_lowercase().ends_with("notepad.exe"));
        assert!(resolve("no_such_program_123.exe").is_none());
    }

    /// 이 PC에 한글이 깔려 있을 때만 의미 있음: cargo test --lib -- --ignored
    #[test]
    #[ignore]
    fn finds_hangul() {
        let p = resolve("Hwp.exe").expect("Hwp.exe 경로를 못 찾음");
        println!("Hwp.exe => {p}");
        assert!(p.to_lowercase().ends_with("hwp.exe"));
    }
}
