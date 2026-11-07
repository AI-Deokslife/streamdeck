//! 프로그램(.exe) 아이콘을 PNG data URL 로 꺼내기 — 버튼 이미지로 쓴다.
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{mem, ptr};
use windows_sys::Win32::{
    Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, HGDIOBJ,
    },
    UI::{
        Shell::SHDefExtractIconW,
        WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO},
    },
};

/// 파일의 아이콘을 `size`px PNG 로. 큰 아이콘이 없으면 있는 크기 그대로.
/// exe/dll/ico 는 바로, 바로가기(.lnk)는 지정된 아이콘 또는 대상 프로그램에서,
/// 인터넷 바로가기(.url — Steam 게임 등)는 IconFile 에서 꺼낸다.
pub fn file_icon_png(path: &str, size: u32) -> Result<Vec<u8>, String> {
    let lower = path.to_lowercase();
    if lower.ends_with(".lnk") {
        let link = resolve_lnk(path).ok_or("바로가기를 읽지 못했어요")?;
        // 바로가기에 따로 지정한 아이콘 → 없거나 실패하면 대상 프로그램 아이콘
        if let Some((icon, index)) = link.icon.filter(|(p, _)| std::path::Path::new(p).is_file()) {
            if let Ok(png) = extract(&icon, index, size) {
                return Ok(png);
            }
        }
        return match link.target {
            Some(t) if std::path::Path::new(&t).is_file() => extract(&t, 0, size),
            _ => Err("바로가기 대상에서 아이콘을 찾지 못했어요".into()),
        };
    }
    if lower.ends_with(".url") {
        let (icon, index) = url_shortcut_icon(path).ok_or("이 인터넷 바로가기에는 아이콘 정보가 없어요")?;
        return extract(&icon, index, size);
    }
    extract(path, 0, size)
}

struct LnkInfo {
    /// 바로가기에 지정된 아이콘 파일과 번호
    icon: Option<(String, i32)>,
    /// 바로가기가 실행하는 파일
    target: Option<String>,
}

/// .lnk 바로가기 읽기 (Windows 셸 COM 사용)
fn resolve_lnk(path: &str) -> Option<LnkInfo> {
    use windows::{
        core::{Interface, HSTRING},
        Win32::{
            System::Com::{CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STGM_READ},
            UI::Shell::{IShellLinkW, ShellLink},
        },
    };
    unsafe {
        // 이미 초기화된 스레드면 실패해도 괜찮다
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = link.cast().ok()?;
        file.Load(&HSTRING::from(path), STGM_READ).ok()?;

        let mut icon_buf = [0u16; 1024];
        let mut index = 0i32;
        let icon = link
            .GetIconLocation(&mut icon_buf, &mut index)
            .ok()
            .map(|_| expand_env(&wide_to_string(&icon_buf)))
            .filter(|s| !s.is_empty())
            .map(|s| (s, index));

        let mut target_buf = [0u16; 1024];
        let target = link
            .GetPath(&mut target_buf, ptr::null_mut(), 0)
            .ok()
            .map(|_| wide_to_string(&target_buf))
            .filter(|s| !s.is_empty());
        Some(LnkInfo { icon, target })
    }
}

/// .url 인터넷 바로가기의 IconFile / IconIndex (INI 형식)
fn url_shortcut_icon(path: &str) -> Option<(String, i32)> {
    let text = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&text);
    let mut file = None;
    let mut index = 0;
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("IconFile=") {
            file = Some(expand_env(v.trim().trim_matches('"')));
        } else if let Some(v) = line.strip_prefix("IconIndex=") {
            index = v.trim().parse().unwrap_or(0);
        }
    }
    file.filter(|f| std::path::Path::new(f).is_file()).map(|f| (f, index))
}

fn wide_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// "%SystemRoot%\..." 같은 환경 변수 풀기
fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        let Some(end) = rest[start + 1..].find('%') else { break };
        let name = &rest[start + 1..start + 1 + end];
        out.push_str(&rest[..start]);
        match std::env::var(name) {
            Ok(v) => out.push_str(&v),
            Err(_) => out.push_str(&rest[start..start + end + 2]),
        }
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    out
}

/// exe/dll/ico 의 `index`번째 아이콘 (음수면 리소스 ID)
fn extract(path: &str, index: i32, size: u32) -> Result<Vec<u8>, String> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut hicon: HICON = ptr::null_mut();
    let hr = unsafe { SHDefExtractIconW(wide.as_ptr(), index, 0, &mut hicon, ptr::null_mut(), size) };
    if hr != 0 || hicon.is_null() {
        return Err("이 파일에서 아이콘을 꺼내지 못했어요".into());
    }
    let result = unsafe { icon_to_rgba(hicon) };
    unsafe { DestroyIcon(hicon) };
    let (w, h, rgba) = result?;
    encode_png(w, h, &rgba)
}

/// HICON → (가로, 세로, RGBA 픽셀)
unsafe fn icon_to_rgba(hicon: HICON) -> Result<(u32, u32, Vec<u8>), String> {
    let mut info: ICONINFO = unsafe { mem::zeroed() };
    if unsafe { GetIconInfo(hicon, &mut info) } == 0 {
        return Err("아이콘 정보를 읽지 못했어요".into());
    }
    let cleanup = |info: &ICONINFO| unsafe {
        if !info.hbmColor.is_null() {
            DeleteObject(info.hbmColor as HGDIOBJ);
        }
        if !info.hbmMask.is_null() {
            DeleteObject(info.hbmMask as HGDIOBJ);
        }
    };
    if info.hbmColor.is_null() {
        cleanup(&info);
        return Err("흑백 아이콘은 지원하지 않아요".into());
    }

    let mut bmp: BITMAP = unsafe { mem::zeroed() };
    unsafe { GetObjectW(info.hbmColor as HGDIOBJ, mem::size_of::<BITMAP>() as i32, (&mut bmp as *mut BITMAP).cast()) };
    let (w, h) = (bmp.bmWidth.max(0) as u32, bmp.bmHeight.max(0) as u32);
    if w == 0 || h == 0 {
        cleanup(&info);
        return Err("아이콘 크기가 0이에요".into());
    }

    let mut bmi: BITMAPINFO = unsafe { mem::zeroed() };
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w as i32,
        biHeight: -(h as i32), // 위에서 아래 순서
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..unsafe { mem::zeroed() }
    };
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let hdc = unsafe { GetDC(ptr::null_mut()) };
    let lines = unsafe {
        GetDIBits(hdc, info.hbmColor, 0, h, pixels.as_mut_ptr().cast(), &mut bmi, DIB_RGB_COLORS)
    };
    unsafe { ReleaseDC(ptr::null_mut(), hdc) };
    cleanup(&info);
    if lines == 0 {
        return Err("아이콘 픽셀을 읽지 못했어요".into());
    }

    // BGRA → RGBA. 옛날 아이콘은 알파가 전부 0이라 그대로면 투명해지므로 불투명 처리
    let has_alpha = pixels.chunks_exact(4).any(|p| p[3] != 0);
    for p in pixels.chunks_exact_mut(4) {
        p.swap(0, 2);
        if !has_alpha {
            p[3] = 255;
        }
    }
    Ok((w, h, pixels))
}

pub fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

pub fn to_data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(png))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_notepad_icon() {
        let path = crate::apppath::resolve("notepad.exe").expect("notepad 경로");
        let png = file_icon_png(&path, 128).expect("아이콘 추출 실패");
        assert_eq!(&png[1..4], b"PNG");
        let url = to_data_url(&png);
        assert!(url.starts_with("data:image/png;base64,"));
        println!("notepad icon: {} bytes", png.len());
    }

    #[test]
    fn expands_environment_variables() {
        let root = std::env::var("SystemRoot").unwrap();
        assert_eq!(expand_env(r"%SystemRoot%\notepad.exe"), format!(r"{root}\notepad.exe"));
        assert_eq!(expand_env(r"%NO_SUCH_VAR_QD%\x"), r"%NO_SUCH_VAR_QD%\x");
        assert_eq!(expand_env("plain"), "plain");
    }

    #[test]
    fn reads_icon_from_internet_shortcut() {
        // Steam 게임 바로가기와 같은 형식의 .url 파일
        let notepad = crate::apppath::resolve("notepad.exe").unwrap();
        let path = std::env::temp_dir().join(format!("qd-test-{}.url", std::process::id()));
        let body = format!("[InternetShortcut]\r\nURL=steam://rungameid/0\r\nIconIndex=0\r\nIconFile={notepad}\r\n");
        std::fs::write(&path, body).unwrap();
        let png = file_icon_png(path.to_str().unwrap(), 64).expect(".url 아이콘 추출 실패");
        assert_eq!(&png[1..4], b"PNG");
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod manual {
    /// 아이콘 눈으로 확인용: ICON_OUT=폴더 cargo test --lib dump_icons -- --ignored
    #[test]
    #[ignore]
    fn dump_icons() {
        let out = std::env::var("ICON_OUT").expect("ICON_OUT");
        for exe in ["Hwp.exe", "chrome.exe", "msedge.exe", "notepad.exe"] {
            if let Some(p) = crate::apppath::resolve(exe) {
                match super::file_icon_png(&p, 128) {
                    Ok(png) => std::fs::write(format!("{out}/{exe}.png"), png).unwrap(),
                    Err(e) => println!("{exe}: {e}"),
                }
            }
        }
    }
}

#[cfg(test)]
mod manual_files {
    /// 경로 목록의 아이콘을 꺼내 저장: ICON_FILES="a.exe|b.lnk" ICON_OUT=폴더 cargo test --lib dump_file_icons -- --ignored
    #[test]
    #[ignore]
    fn dump_file_icons() {
        let out = std::env::var("ICON_OUT").expect("ICON_OUT");
        for (i, f) in std::env::var("ICON_FILES").expect("ICON_FILES").split('|').enumerate() {
            match super::file_icon_png(f, 128) {
                Ok(png) => {
                    std::fs::write(format!("{out}/file{i}.png"), &png).unwrap();
                    println!("{f}: {} bytes", png.len());
                }
                Err(e) => println!("{f}: 실패 {e}"),
            }
        }
    }
}
