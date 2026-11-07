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

/// exe(또는 .ico/.dll) 파일의 첫 번째 아이콘을 `size`px PNG 로. 큰 아이콘이 없으면 있는 크기 그대로.
pub fn file_icon_png(path: &str, size: u32) -> Result<Vec<u8>, String> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut hicon: HICON = ptr::null_mut();
    let hr = unsafe { SHDefExtractIconW(wide.as_ptr(), 0, 0, &mut hicon, ptr::null_mut(), size) };
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
