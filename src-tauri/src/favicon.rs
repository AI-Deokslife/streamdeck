//! 웹사이트 아이콘 가져오기 (URL 버튼 이미지용).
//! 그 사이트에만 직접 접속한다: 페이지의 <link rel="apple-touch-icon"|"icon"> → 없으면 /favicon.ico
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use std::time::Duration;
use ureq::ResponseExt;
use url::Url;

const MAX_PAGE: u64 = 2 * 1024 * 1024;
const MAX_ICON: u64 = 1024 * 1024;
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) QuickDeck/0.2";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteIcon {
    /// data:image/...;base64,...
    pub data_url: String,
    /// 큰 정사각형 아이콘(apple-touch-icon)이면 버튼 전체를 채우는 게 보기 좋다
    pub full: bool,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(6)))
        .build()
        .into()
}

pub fn fetch(page_url: &str) -> Result<SiteIcon, String> {
    let mut page = page_url.trim().to_string();
    if !page.contains("://") {
        page = format!("https://{page}");
    }
    let base = Url::parse(&page).map_err(|_| "웹 주소 형식이 아니에요".to_string())?;
    if !matches!(base.scheme(), "http" | "https") {
        return Err("http/https 주소만 아이콘을 가져올 수 있어요".into());
    }
    let agent = agent();

    // 1) 페이지에서 아이콘 링크 찾기 (실패해도 /favicon.ico 로 넘어감)
    let mut candidates: Vec<(Url, bool)> = Vec::new();
    let mut final_base = base.clone();
    if let Ok(mut res) = agent.get(base.as_str()).header("User-Agent", USER_AGENT).call() {
        // 리다이렉트된 최종 주소 기준으로 상대 경로를 푼다
        if let Ok(u) = Url::parse(&res.get_uri().to_string()) {
            final_base = u;
        }
        if let Ok(bytes) = res.body_mut().with_config().limit(MAX_PAGE).read_to_vec() {
            let html = String::from_utf8_lossy(&bytes);
            for link in icon_links(&html) {
                if let Ok(u) = final_base.join(&link.href) {
                    candidates.push((u, link.touch));
                }
            }
        }
    }
    // 2) 기본 위치
    if let Ok(u) = final_base.join("/favicon.ico") {
        candidates.push((u, false));
    }

    for (url, touch) in candidates {
        if let Ok((mime, bytes)) = download_image(&agent, &url) {
            return Ok(SiteIcon { data_url: format!("data:{mime};base64,{}", STANDARD.encode(&bytes)), full: touch });
        }
    }
    Err("이 사이트의 아이콘을 찾지 못했어요".into())
}

fn download_image(agent: &ureq::Agent, url: &Url) -> Result<(String, Vec<u8>), String> {
    let mut res = agent
        .get(url.as_str())
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| e.to_string())?;
    let header_mime = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or("").trim().to_lowercase())
        .unwrap_or_default();
    let bytes = res.body_mut().with_config().limit(MAX_ICON).read_to_vec().map_err(|e| e.to_string())?;
    let mime = sniff_mime(&bytes).map(str::to_string).or_else(|| {
        // 서버가 알려준 형식은 이미지일 때만 믿는다 (HTML 오류 페이지 거르기)
        header_mime.starts_with("image/").then_some(header_mime)
    });
    match mime {
        Some(m) if !bytes.is_empty() => Ok((m, bytes)),
        _ => Err("이미지가 아님".into()),
    }
}

/// 파일 앞부분으로 이미지 형식 알아내기
fn sniff_mime(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(b"\x89PNG") {
        Some("image/png")
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if b.starts_with(b"GIF8") {
        Some("image/gif")
    } else if b.starts_with(&[0, 0, 1, 0]) {
        Some("image/x-icon")
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        let head = String::from_utf8_lossy(&b[..b.len().min(512)]).to_lowercase();
        (head.contains("<svg")).then_some("image/svg+xml")
    }
}

#[derive(Debug, PartialEq)]
struct IconLink {
    href: String,
    /// apple-touch-icon (보통 180px 정사각형)
    touch: bool,
    size: u32,
}

/// HTML 에서 아이콘 <link> 들을 좋은 순서(큰 터치 아이콘 → 큰 아이콘)로
fn icon_links(html: &str) -> Vec<IconLink> {
    let lower = html.to_lowercase();
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(start) = lower[pos..].find("<link") {
        let s = pos + start;
        let Some(end) = lower[s..].find('>') else { break };
        let tag = &html[s..s + end];
        let tag_l = &lower[s..s + end];
        pos = s + end;
        let Some(rel) = attr(tag, tag_l, "rel") else { continue };
        let rel = rel.to_lowercase();
        if !rel.split_whitespace().any(|r| r == "icon" || r == "apple-touch-icon" || r == "apple-touch-icon-precomposed")
        {
            continue;
        }
        let Some(href) = attr(tag, tag_l, "href") else { continue };
        let size = attr(tag, tag_l, "sizes")
            .and_then(|s| s.split(['x', 'X']).next().and_then(|n| n.trim().parse().ok()))
            .unwrap_or(if rel.contains("apple") { 180 } else { 16 });
        out.push(IconLink { href: href.to_string(), touch: rel.contains("apple-touch-icon"), size });
    }
    out.sort_by(|a, b| b.touch.cmp(&a.touch).then(b.size.cmp(&a.size)));
    out
}

/// 태그에서 속성 값 꺼내기 (따옴표 " ' 또는 없음)
fn attr<'a>(tag: &'a str, tag_lower: &str, name: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(i) = tag_lower[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        // 다른 속성 이름의 일부가 아닌지 (예: data-rel)
        if at > 0 && !tag_lower.as_bytes()[at - 1].is_ascii_whitespace() {
            continue;
        }
        let rest = tag_lower[from..].trim_start();
        if !rest.starts_with('=') {
            continue;
        }
        let vstart = tag.len() - rest.len() + 1;
        let v = tag[vstart..].trim_start();
        let offset = tag.len() - v.len();
        return Some(match v.chars().next()? {
            q @ ('"' | '\'') => {
                let close = v[1..].find(q)?;
                &tag[offset + 1..offset + 1 + close]
            }
            _ => {
                // 따옴표 없는 값은 공백까지 (경로의 / 는 값의 일부)
                let len = v.find(char::is_whitespace).unwrap_or(v.len());
                &tag[offset..offset + len]
            }
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_best_icon_links() {
        let html = r#"<html><head>
            <link rel="stylesheet" href="/a.css">
            <link rel="icon" href="/fav-32.png" sizes="32x32">
            <LINK REL="shortcut icon" HREF='/favicon.ico'>
            <link data-rel="x" rel=icon href=/fav-192.png sizes=192x192 />
            <link rel="apple-touch-icon" href="https://cdn.example.com/touch.png">
        </head></html>"#;
        let links = icon_links(html);
        let hrefs: Vec<_> = links.iter().map(|l| l.href.as_str()).collect();
        assert_eq!(hrefs, ["https://cdn.example.com/touch.png", "/fav-192.png", "/fav-32.png", "/favicon.ico"]);
        assert!(links[0].touch);
    }

    #[test]
    fn sniffs_image_types() {
        assert_eq!(sniff_mime(b"\x89PNG\r\n"), Some("image/png"));
        assert_eq!(sniff_mime(&[0, 0, 1, 0, 1]), Some("image/x-icon"));
        assert_eq!(sniff_mime(b"<?xml?><svg xmlns="), Some("image/svg+xml"));
        assert_eq!(sniff_mime(b"<!doctype html><html>"), None);
    }

    #[test]
    fn rejects_non_web_urls() {
        assert!(fetch("file:///C:/Windows").is_err());
    }

    /// 인터넷 필요: cargo test --lib real_site_icon -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_site_icon() {
        for site in ["https://www.naver.com", "github.com", "https://www.youtube.com"] {
            match fetch(site) {
                Ok(i) => println!("{site}: {} bytes, full={}", i.data_url.len(), i.full),
                Err(e) => panic!("{site}: {e}"),
            }
        }
    }
}
