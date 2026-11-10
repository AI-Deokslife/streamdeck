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

/// 사진을 읽고, 사진 안의 방향 정보(EXIF)대로 돌려 세운다.
/// 휴대폰은 세로로 찍어도 가로로 저장하고 '90도 돌려서 보여 주세요'만 적어 두기 때문.
pub fn load_upright(path: &Path) -> Result<image::RgbaImage, String> {
    use image::{DynamicImage, ImageDecoder, ImageReader};
    let err = |e: image::ImageError| format!("사진을 읽지 못했어요: {e}");
    let mut decoder = ImageReader::open(path)
        .map_err(|e| format!("사진을 열지 못했어요: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("사진을 열지 못했어요: {e}"))?
        .into_decoder()
        .map_err(err)?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).map_err(err)?;
    img.apply_orientation(orientation);
    Ok(img.to_rgba8())
}

/// 여러 파일을 클립보드에 한꺼번에 넣는다. 받는 프로그램마다 읽는 형식이 달라서 세 가지를 함께 넣는다.
/// - 파일 묶음: 메신저·탐색기·Claude 등은 모든 파일을 받는다
/// - 그림 한 장(`picture`): 카카오톡·한글처럼 그림만 읽는 프로그램용 ('다음 사진'으로 바꿔 가며 붙인다)
/// - 웹 문서(HTML, `with_html`): 사진들을 `<img>` 로 나열 — 웹 문서를 붙여 넣을 줄 아는 프로그램용
pub fn copy_batch_to_clipboard(paths: &[PathBuf], picture: Option<&Path>, with_html: bool) -> Result<(), String> {
    use clipboard_win::{options::NoClear, raw, Clipboard};
    // 그림은 클립보드를 열기 전에 미리 만들어 둔다 (클립보드를 오래 붙잡지 않게)
    let bmp = picture.and_then(|p| load_upright(p).ok()).map(|img| to_bmp(&img));
    let names: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
    let html = with_html.then(|| images_html(paths)).filter(|h| !h.is_empty());
    let _open = Clipboard::new_attempts(10).map_err(|e| format!("클립보드를 열지 못했어요: {e}"))?;
    raw::empty().map_err(|e| format!("클립보드를 비우지 못했어요: {e}"))?;
    raw::set_file_list_with(&names, NoClear).map_err(|e| format!("클립보드에 넣지 못했어요: {e}"))?;
    // 그림·웹 문서는 못 넣어도 파일 묶음은 이미 들어갔으니 실패로 치지 않는다
    if let Some(bmp) = bmp {
        let _ = raw::set_bitmap_with(&bmp, NoClear);
    }
    if let (Some(html), Some(fmt)) = (html, raw::register_format("HTML Format")) {
        let _ = raw::set_html_with(fmt.get(), &html, NoClear);
    }
    Ok(())
}

/// 사진 파일들을 웹 문서 조각으로: <img src="file:///C:/...">
/// 한글에 직접 Ctrl+V 하면 그림을 하나씩 앞에 끼워 넣어서 순서가 거꾸로 된다(실제로 확인).
/// 그래서 뒤에서부터 적어 둔다 → 한글에는 보낸 순서대로 들어간다.
pub fn images_html(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .rev()
        .filter(|p| is_clip_image(p))
        .map(|p| {
            let url = p.display().to_string().replace('\\', "/").replace('%', "%25").replace(' ', "%20").replace('#', "%23");
            format!("<p><img src=\"file:///{url}\"></p>")
        })
        .collect::<Vec<_>>()
        .join("")
}

/// RGBA 그림 → 24비트 BMP 파일 내용 (클립보드 그림 형식에 쓰인다)
pub fn to_bmp(img: &image::RgbaImage) -> Vec<u8> {
    let (w, h) = img.dimensions();
    let row = (w as usize * 3).div_ceil(4) * 4; // 한 줄은 4바이트 단위로 맞춘다
    let data_len = row * h as usize;
    let mut out = Vec::with_capacity(54 + data_len);
    // 파일 머리 (14바이트)
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((54 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&54u32.to_le_bytes());
    // 정보 머리 (40바이트)
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes()); // 양수 = 아래 줄부터
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    for y in (0..h).rev() {
        let start = out.len();
        for x in 0..w {
            let p = img.get_pixel(x, y).0;
            out.extend_from_slice(&[p[2], p[1], p[0]]);
        }
        out.resize(start + row, 0);
    }
    out
}

/// 사진을 클립보드에 넣는다 (방향을 바로 세워서)
pub fn copy_image_to_clipboard(path: &Path) -> Result<(), String> {
    let img = load_upright(path)?;
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

    /// 방향 정보(Orientation=6, 시계 방향 90도)가 든 EXIF 조각을 JPEG 앞부분에 끼워 넣는다
    fn with_exif_orientation(jpeg: &[u8], orientation: u8) -> Vec<u8> {
        let tiff: Vec<u8> = vec![
            b'M', b'M', 0, 42, 0, 0, 0, 8, // 빅엔디언 TIFF 머리, 첫 IFD 는 8번째 바이트
            0, 1, // 항목 1개
            0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, orientation, 0, 0, // Orientation(0x0112), SHORT, 1개, 값
            0, 0, 0, 0, // 다음 IFD 없음
        ];
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend_from_slice(&tiff);
        let len = (app1.len() + 2) as u16;
        let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&app1);
        out.extend_from_slice(&jpeg[2..]); // 원래 SOI(FFD8) 뒤부터
        out
    }

    #[test]
    fn portrait_photos_stand_upright() {
        let dir = std::env::temp_dir().join(format!("qd-orient-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        // 가로 8×4 로 저장됐지만 '90도 돌려서 보기'가 적힌 사진 → 세로 4×8 로 읽혀야 한다
        let plain = dir.join("plain.jpg");
        image::RgbImage::from_pixel(8, 4, image::Rgb([10, 20, 30])).save(&plain).unwrap();
        let rotated = dir.join("rotated.jpg");
        fs::write(&rotated, with_exif_orientation(&fs::read(&plain).unwrap(), 6)).unwrap();
        assert_eq!(load_upright(&plain).unwrap().dimensions(), (8, 4));
        assert_eq!(load_upright(&rotated).unwrap().dimensions(), (4, 8));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn html_lists_only_photos() {
        let files = vec![PathBuf::from(r"C:\받은 파일\사진 1.jpg"), PathBuf::from(r"C:\a\회의록.pdf"), PathBuf::from(r"C:\a\b.png")];
        assert_eq!(images_html(&files), "<p><img src=\"file:///C:/a/b.png\"></p><p><img src=\"file:///C:/받은%20파일/사진%201.jpg\"></p>", "뒤에서부터");
    }

    /// 손으로 돌리는 실험: QD_CLIPTEST=사진1;사진2 cargo test --lib -- --ignored clipboard_experiment
    /// (실제 클립보드를 덮어쓴다)
    #[test]
    #[ignore]
    fn clipboard_experiment() {
        let list = std::env::var("QD_CLIPTEST").expect("QD_CLIPTEST 에 사진 경로를 ; 로 이어서");
        let files: Vec<PathBuf> = list.split(';').map(PathBuf::from).collect();
        let html = std::env::var("QD_CLIPTEST_HTML").map(|v| v != "0").unwrap_or(true);
        let pic = std::env::var("QD_CLIPTEST_PIC").map(|v| v != "0").unwrap_or(true);
        copy_batch_to_clipboard(&files, files.first().filter(|_| pic).map(|p| p.as_path()), html).unwrap();
    }

    #[test]
    fn bmp_for_clipboard() {
        // 3×2, 한 줄 9바이트 → 12바이트로 맞춤, 아래 줄부터 BGR
        let mut img = image::RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255]));
        img.put_pixel(0, 1, image::Rgba([10, 20, 30, 255]));
        let b = to_bmp(&img);
        assert_eq!(&b[0..2], b"BM");
        assert_eq!(b.len(), 54 + 12 * 2);
        assert_eq!(u32::from_le_bytes(b[2..6].try_into().unwrap()) as usize, b.len());
        assert_eq!(&b[54..57], &[30, 20, 10], "맨 아래 줄(y=1)의 첫 점이 먼저, BGR 순서");
        assert_eq!(&b[66..69], &[3, 2, 1]);
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
