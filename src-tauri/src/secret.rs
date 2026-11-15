//! 비밀 텍스트(비밀번호 등)를 파일에 쓸 때 Windows 계정으로만 풀 수 있게 잠근다 (DPAPI).
//! 메모리와 화면에서는 원래 글자 그대로 쓰고, config.json·백업·내보내기 파일에만 잠긴 모양으로 남는다.
//! 다른 Windows 계정이나 다른 PC에서는 풀 수 없다.
use base64::{engine::general_purpose::STANDARD, Engine};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB},
};

use crate::config::{Action, Button, Config};

/// 잠긴 글자 앞에 붙는 표시
const PREFIX: &str = "dpapi:";
/// 잠글 때 같이 섞는 값 (다른 프로그램이 같은 방식으로 만든 데이터와 섞이지 않게)
const ENTROPY: &[u8] = b"QuickDeck secret text v1";

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
}

/// DPAPI 결과 버퍼를 Vec 로 옮기고 해제한다
unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    unsafe {
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as _);
        v
    }
}

/// 글자 → "dpapi:<base64>"
pub fn protect(text: &str) -> Result<String, String> {
    let input = blob(text.as_bytes());
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe {
        CryptProtectData(&input, std::ptr::null(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
    };
    if ok == 0 {
        return Err("비밀 텍스트를 잠그지 못했어요".into());
    }
    Ok(format!("{PREFIX}{}", STANDARD.encode(unsafe { take(out) })))
}

/// "dpapi:<base64>" → 글자. 잠긴 모양이 아니면 그대로 돌려준다 (예전 설정, 직접 쓴 파일)
pub fn unprotect(stored: &str) -> Result<String, String> {
    let Some(b64) = stored.strip_prefix(PREFIX) else {
        return Ok(stored.to_string());
    };
    let data = STANDARD.decode(b64.trim()).map_err(|_| "비밀 텍스트가 손상됐어요".to_string())?;
    let input = blob(&data);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe {
        CryptUnprotectData(&input, std::ptr::null_mut(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
    };
    if ok == 0 {
        return Err("다른 PC나 다른 Windows 계정에서 만든 비밀 텍스트라 풀 수 없어요".into());
    }
    String::from_utf8(unsafe { take(out) }).map_err(|_| "비밀 텍스트가 손상됐어요".to_string())
}

/// 버튼 안의 모든 동작(멀티 단계, 번갈아, 길게 누르기, 폴더 안까지)에 f 를 적용
fn walk_actions(buttons: &mut [Button], f: &mut dyn FnMut(&mut Action)) {
    fn act(a: &mut Action, f: &mut dyn FnMut(&mut Action)) {
        match a {
            Action::Multi { steps } => steps.iter_mut().for_each(|s| act(s, f)),
            Action::Toggle { first, second, .. } => {
                act(first, f);
                act(second, f);
            }
            Action::Folder { buttons, .. } => walk_actions(buttons, f),
            _ => f(a),
        }
    }
    for b in buttons {
        act(&mut b.action, f);
        if let Some(l) = &mut b.long_action {
            act(l, f);
        }
    }
}

/// 파일에 쓸 모양: 비밀 텍스트만 잠근 복사본
pub fn sealed(cfg: &Config) -> Result<Config, String> {
    let mut out = cfg.clone();
    let mut err = None;
    walk_actions(&mut out.buttons, &mut |a| {
        if let Action::Text { text, secret: true, .. } = a {
            if !text.starts_with(PREFIX) {
                match protect(text) {
                    Ok(t) => *text = t,
                    Err(e) => err = Some(e),
                }
            }
        }
    });
    match err {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

/// 파일에서 읽은 설정의 비밀 텍스트를 푼다. 풀지 못한 것은 비워 두고 그 개수를 돌려준다
pub fn unseal(cfg: &mut Config) -> usize {
    let mut failed = 0;
    walk_actions(&mut cfg.buttons, &mut |a| {
        if let Action::Text { text, secret: true, .. } = a {
            match unprotect(text) {
                Ok(t) => *text = t,
                Err(_) => {
                    text.clear();
                    failed += 1;
                }
            }
        }
    });
    failed
}

/// 이 동작(안쪽 단계 포함)에 비밀 텍스트가 들어 있나
pub fn has_secret(a: &Action) -> bool {
    match a {
        Action::Text { secret, .. } => *secret,
        Action::Multi { steps } => steps.iter().any(has_secret),
        Action::Toggle { first, second, .. } => has_secret(first) || has_secret(second),
        _ => false,
    }
}

/// 버튼(누르기·길게 누르기)에 비밀 텍스트가 들어 있나
pub fn button_has_secret(b: &Button) -> bool {
    has_secret(&b.action) || b.long_action.as_ref().is_some_and(has_secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protect_roundtrip() {
        let locked = protect("비밀번호123!@#").unwrap();
        assert!(locked.starts_with(PREFIX));
        assert!(!locked.contains("비밀번호"));
        assert_eq!(unprotect(&locked).unwrap(), "비밀번호123!@#");
        // 잠기지 않은 글자는 그대로
        assert_eq!(unprotect("그냥 글자").unwrap(), "그냥 글자");
        // 망가진 데이터는 실패
        assert!(unprotect("dpapi:AAAA").is_err());
    }

    #[test]
    fn config_seal_unseal() {
        let json = r##"{"toggleHotkey":"","columns":4,"buttons":[
            {"id":"pw","label":"로그인","action":{"type":"multi","steps":[
                {"type":"text","text":"myid"},
                {"type":"keys","keys":"tab"},
                {"type":"text","text":"p@ss워드","secret":true}
            ]}},
            {"id":"f","label":"폴더","action":{"type":"folder","buttons":[
                {"id":"s","label":"비번","action":{"type":"text","text":"in-folder","secret":true}}
            ]}},
            {"id":"plain","label":"인사","action":{"type":"text","text":"안녕하세요"}}
        ]}"##;
        let cfg = crate::config::parse(json).unwrap();
        let file = serde_json::to_string(&sealed(&cfg).unwrap()).unwrap();
        assert!(!file.contains("p@ss워드") && !file.contains("in-folder"), "파일에 글자 그대로 남으면 안 됨");
        assert!(file.contains("myid") && file.contains("안녕하세요"), "일반 텍스트는 그대로");
        let (back, failed) = crate::config::parse_full(&file).unwrap();
        assert_eq!(failed, 0);
        assert_eq!(serde_json::to_string(&back).unwrap(), serde_json::to_string(&cfg).unwrap());
        assert!(button_has_secret(&back.buttons[0]));
        assert!(!button_has_secret(&back.buttons[2]));
    }

    #[test]
    fn unreadable_secret_is_cleared() {
        let json = r##"{"toggleHotkey":"","columns":4,"buttons":[
            {"id":"pw","label":"비번","action":{"type":"text","text":"dpapi:AAAA","secret":true}}]}"##;
        let (cfg, failed) = crate::config::parse_full(json).unwrap();
        assert_eq!(failed, 1);
        assert!(matches!(&cfg.buttons[0].action, Action::Text { text, .. } if text.is_empty()));
    }
}
