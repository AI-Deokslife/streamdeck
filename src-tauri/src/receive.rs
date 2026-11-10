//! 휴대폰 → PC 파일 받기: 휴대폰 리모컨 '보내기' 탭에서 올린 사진·파일을 정해 둔 폴더에 저장한다.
//! 사진 한 장이면 클립보드에도 넣어서 한글·메신저에 바로 붙여 넣을 수 있게 한다.
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

/// 한 번에 받을 수 있는 가장 큰 파일 (휴대폰 동영상 몇 분 정도)
pub const MAX_BYTES: u64 = 500 * 1024 * 1024;

/// 휴대폰에서 온 파일 이름을 Windows 에서 쓸 수 있게 다듬는다.
/// 경로 기호·금지 문자는 빼고, 카메라가 붙이는 뜻 없는 이름(image.jpg 등)은 날짜 이름으로 바꾼다.
pub fn safe_name(raw: &str, stamp: &str) -> String {
    // 폴더 경로가 섞여 와도 마지막 이름만
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("").trim();
    let cleaned: String = base
        .chars()
        .map(|c| if "<>:\"|?*".contains(c) || c.is_control() { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim_matches(|c: char| c == '.' || c == ' ').to_string();
    let (stem, ext) = match cleaned.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && !e.is_empty() && e.len() <= 8 => (s.to_string(), e.to_lowercase()),
        _ => (cleaned.clone(), String::new()),
    };
    // 휴대폰 카메라가 붙이는 숫자만 있는 이름(1731219000000.jpg)도 뜻이 없다
    let digits_only = stem.len() >= 8 && stem.chars().all(|c| c.is_ascii_digit());
    let meaningless = stem.is_empty()
        || digits_only
        || ["image", "photo", "blob", "file", "scan", "camera"].contains(&stem.to_lowercase().as_str());
    let stem = if meaningless {
        let kind = if stem.eq_ignore_ascii_case("scan") {
            "스캔"
        } else if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "heic" | "webp") {
            "사진"
        } else {
            "파일"
        };
        format!("{kind}_{stamp}")
    } else {
        stem.chars().take(120).collect()
    };
    // Windows 예약 이름(CON, NUL …) 피하기
    let reserved = ["con", "prn", "aux", "nul", "com1", "com2", "com3", "lpt1", "lpt2", "lpt3"];
    let stem = if reserved.contains(&stem.to_lowercase().as_str()) { format!("_{stem}") } else { stem };
    if ext.is_empty() { stem } else { format!("{stem}.{ext}") }
}

/// 같은 이름이 있으면 '이름 (1).jpg', '이름 (2).jpg' …
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (name.to_string(), String::new()),
    };
    (1..10_000)
        .map(|i| dir.join(format!("{stem} ({i}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// 받은 내용을 임시 파일에 쓰고, 다 받으면 진짜 이름으로 바꾼다 (중간에 끊기면 반쪽 파일이 남지 않게)
pub fn save_stream(dir: &Path, name: &str, body: &mut dyn Read) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| format!("저장 폴더를 만들지 못했어요: {e}"))?;
    let target = unique_path(dir, name);
    let tmp = target.with_extension(format!(
        "{}quickdeck-part",
        target.extension().map(|e| format!("{}.", e.to_string_lossy())).unwrap_or_default()
    ));
    let result = (|| -> io::Result<u64> {
        let mut out = fs::File::create(&tmp)?;
        let n = io::copy(&mut body.take(MAX_BYTES + 1), &mut out)?;
        out.flush()?;
        Ok(n)
    })();
    match result {
        Ok(n) if n > MAX_BYTES => {
            let _ = fs::remove_file(&tmp);
            Err(format!("파일이 너무 커요 (최대 {}MB)", MAX_BYTES / 1024 / 1024))
        }
        Ok(0) => {
            let _ = fs::remove_file(&tmp);
            Err("빈 파일이에요".into())
        }
        Ok(_) => {
            fs::rename(&tmp, &target).map_err(|e| format!("파일을 저장하지 못했어요: {e}"))?;
            Ok(target)
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(format!("받는 중에 끊겼어요: {e}"))
        }
    }
}

/// 클립보드에 넣을 수 있는 사진인지 (확장자로)
pub fn is_clip_image(path: &Path) -> bool {
    matches!(
        path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref(),
        Some("jpg" | "jpeg" | "png")
    )
}

/// 사진을 클립보드에 넣는다
pub fn copy_image_to_clipboard(path: &Path) -> Result<(), String> {
    let img = image::open(path).map_err(|e| format!("사진을 읽지 못했어요: {e}"))?.to_rgba8();
    let (w, h) = img.dimensions();
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("클립보드를 열지 못했어요: {e}"))?;
    cb.set_image(arboard::ImageData { width: w as usize, height: h as usize, bytes: img.into_raw().into() })
        .map_err(|e| format!("클립보드에 넣지 못했어요: {e}"))
}

/// 파일 이름에 붙일 날짜·시각 (예: 2026-11-10_143012)
pub fn stamp_now() -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut t = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut t) };
    format!("{:04}-{:02}-{:02}_{:02}{:02}{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cleaned() {
        let s = "2026-11-10_143012";
        assert_eq!(safe_name("image.jpg", s), "사진_2026-11-10_143012.jpg");
        assert_eq!(safe_name("IMG_1234.JPG", s), "IMG_1234.jpg");
        assert_eq!(safe_name("1731219000000.jpg", s), "사진_2026-11-10_143012.jpg");
        assert_eq!(safe_name("scan.jpg", s), "스캔_2026-11-10_143012.jpg");
        assert_eq!(safe_name("..\\..\\Windows\\evil.exe", s), "evil.exe");
        assert_eq!(safe_name("a/b/회의록:최종?.hwp", s), "회의록_최종_.hwp");
        assert_eq!(safe_name("", s), "파일_2026-11-10_143012");
        assert_eq!(safe_name("con.txt", s), "_con.txt");
        assert_eq!(safe_name("...", s), "파일_2026-11-10_143012");
    }

    #[test]
    fn reads_phone_photos() {
        // 클립보드에 넣기 전에 jpg·png 를 읽을 수 있어야 한다 (image crate 의 jpeg 기능이 켜졌는지)
        let dir = std::env::temp_dir().join(format!("qd-img-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for ext in ["jpg", "png"] {
            let p = dir.join(format!("t.{ext}"));
            image::RgbImage::from_pixel(8, 6, image::Rgb([200, 100, 50])).save(&p).unwrap();
            assert!(is_clip_image(&p));
            assert_eq!(image::open(&p).unwrap().to_rgba8().dimensions(), (8, 6));
        }
        assert!(!is_clip_image(Path::new("a.pdf")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saves_without_overwriting() {
        let dir = std::env::temp_dir().join(format!("qd-recv-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let a = save_stream(&dir, "메모.txt", &mut "첫째".as_bytes()).unwrap();
        let b = save_stream(&dir, "메모.txt", &mut "둘째".as_bytes()).unwrap();
        assert_eq!(a.file_name().unwrap(), "메모.txt");
        assert_eq!(b.file_name().unwrap(), "메모 (1).txt");
        assert_eq!(fs::read_to_string(&b).unwrap(), "둘째");
        assert!(save_stream(&dir, "빈.txt", &mut "".as_bytes()).is_err());
        // 임시 파일이 남지 않는다
        assert!(fs::read_dir(&dir).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains("quickdeck-part")));
        let _ = fs::remove_dir_all(&dir);
    }
}
