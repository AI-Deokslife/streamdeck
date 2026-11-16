//! 휴대폰 리모컨 ↔ PC 사이의 내용을 QR 열쇠로 암호화한다 (NaCl secretbox = XSalsa20-Poly1305).
//!
//! - 열쇠(token)는 QR 주소의 `#` 뒤에만 있어서 네트워크로 나가지 않는다.
//! - 실제 암호 열쇠 = SHA-512("quickdeck-remote-v1:" + token) 의 앞 32바이트 (휴대폰도 똑같이 만든다).
//! - 요청마다 무작위 24바이트 nonce 를 쓰고, 같은 nonce 나 오래된 시각의 요청은 거절해서
//!   엿본 요청을 다시 보내는 공격(재전송)을 막는다.
//! - 휴대폰 쪽은 tweetnacl-js(공개 도메인)의 secretbox 와 같은 형식: nonce(24) + tag(16) + 암호문.
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use crypto_secretbox::{
    aead::{Aead, KeyInit},
    Nonce, XSalsa20Poly1305,
};
use sha2::{Digest, Sha512};
use std::{
    collections::HashMap,
    io::{self, Read},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

/// 요청 시각이 PC 시계와 이만큼(초) 넘게 차이 나면 거절
pub const MAX_SKEW: u64 = 600;
/// 파일 조각 하나의 최대 크기 (휴대폰이 1MB 씩 잘라 보낸다)
pub const MAX_FRAME: usize = 1024 * 1024 + 16;

pub fn cipher(token: &str) -> XSalsa20Poly1305 {
    let h = Sha512::digest(format!("quickdeck-remote-v1:{token}").as_bytes());
    XSalsa20Poly1305::new_from_slice(&h[..32]).expect("32바이트 열쇠")
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// 평문 → base64(nonce + 암호문)
pub fn seal(c: &XSalsa20Poly1305, plain: &[u8]) -> String {
    let mut nonce = [0u8; 24];
    getrandom(&mut nonce);
    let ct = c.encrypt(Nonce::from_slice(&nonce), plain).expect("암호화");
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ct);
    STANDARD.encode(out)
}

/// base64(nonce + 암호문) → (nonce, 평문). 열쇠가 다르거나 내용이 바뀌었으면 None.
/// 표준·URL용 base64 둘 다 받는다
pub fn open(c: &XSalsa20Poly1305, text: &str) -> Option<([u8; 24], Vec<u8>)> {
    let text = text.trim();
    let data = STANDARD.decode(text).or_else(|_| URL_SAFE_NO_PAD.decode(text)).ok()?;
    if data.len() < 24 + 16 {
        return None;
    }
    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&data[..24]);
    let plain = c.decrypt(Nonce::from_slice(&nonce), &data[24..]).ok()?;
    Some((nonce, plain))
}

fn getrandom(buf: &mut [u8]) {
    // Windows 의 암호용 난수 (BCryptGenRandom)
    use windows_sys::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
    let ok = unsafe { BCryptGenRandom(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    assert_eq!(ok, 0, "난수를 만들지 못했어요");
}

static SEEN: Mutex<Option<HashMap<[u8; 24], u64>>> = Mutex::new(None);

/// 처음 보는 nonce 이고 시각이 맞으면 Ok. (본 nonce 는 MAX_SKEW*2 동안 기억)
pub fn check_fresh(nonce: [u8; 24], ts: u64) -> Result<(), &'static str> {
    let now = now_secs();
    if ts.abs_diff(now) > MAX_SKEW {
        return Err("PC 와 휴대폰의 시계가 10분 넘게 달라요. 휴대폰의 날짜·시간을 '자동'으로 맞춰 주세요");
    }
    let mut g = SEEN.lock().unwrap();
    let seen = g.get_or_insert_with(HashMap::new);
    if seen.len() > 20_000 {
        seen.retain(|_, t| now.saturating_sub(*t) <= MAX_SKEW * 2);
    }
    if seen.insert(nonce, now).is_some() {
        return Err("이미 처리한 요청이에요");
    }
    Ok(())
}

/// 암호화된 파일 조각들을 풀어 주는 Read.
/// 조각 = [암호문 길이 4바이트(빅엔디언)][암호문]. 조각 i 의 nonce = 머리 nonce 앞 16바이트 + i(8바이트, 빅엔디언)
/// 이라서 조각의 순서를 바꾸거나 다른 파일의 조각을 끼워 넣을 수 없고, 정해진 크기보다 짧으면 오류로 끝난다.
pub struct FrameReader<'a, R: Read> {
    inner: R,
    cipher: &'a XSalsa20Poly1305,
    base: [u8; 16],
    index: u64,
    buf: Vec<u8>,
    pos: usize,
    remaining: u64,
}

impl<'a, R: Read> FrameReader<'a, R> {
    pub fn new(inner: R, cipher: &'a XSalsa20Poly1305, head_nonce: [u8; 24], size: u64) -> Self {
        let mut base = [0u8; 16];
        base.copy_from_slice(&head_nonce[..16]);
        Self { inner, cipher, base, index: 0, buf: Vec::new(), pos: 0, remaining: size }
    }

    fn next_frame(&mut self) -> io::Result<()> {
        let mut len = [0u8; 4];
        self.inner.read_exact(&mut len)?;
        let len = u32::from_be_bytes(len) as usize;
        if !(16..=MAX_FRAME).contains(&len) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "조각 크기가 이상해요"));
        }
        let mut ct = vec![0u8; len];
        self.inner.read_exact(&mut ct)?;
        let mut nonce = [0u8; 24];
        nonce[..16].copy_from_slice(&self.base);
        nonce[16..].copy_from_slice(&self.index.to_be_bytes());
        let plain = self
            .cipher
            .decrypt(Nonce::from_slice(&nonce), ct.as_slice())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "파일 내용이 바뀌었어요"))?;
        if plain.len() as u64 > self.remaining {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "파일이 알려 준 크기보다 커요"));
        }
        self.remaining -= plain.len() as u64;
        self.index += 1;
        self.buf = plain;
        self.pos = 0;
        Ok(())
    }
}

impl<R: Read> Read for FrameReader<'_, R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.pos >= self.buf.len() {
            if self.remaining == 0 {
                return Ok(0);
            }
            self.next_frame()?;
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// 테스트용: 휴대폰처럼 파일을 조각으로 암호화
    pub fn frames(c: &XSalsa20Poly1305, head_nonce: [u8; 24], data: &[u8], chunk: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, part) in data.chunks(chunk).enumerate() {
            let mut nonce = [0u8; 24];
            nonce[..16].copy_from_slice(&head_nonce[..16]);
            nonce[16..].copy_from_slice(&(i as u64).to_be_bytes());
            let ct = c.encrypt(Nonce::from_slice(&nonce), part).unwrap();
            out.extend_from_slice(&(ct.len() as u32).to_be_bytes());
            out.extend_from_slice(&ct);
        }
        out
    }

    #[test]
    fn same_as_tweetnacl() {
        // tweetnacl-js: nacl.secretbox(utf8("hello"), [0..23], sha512("quickdeck-remote-v1:<token>")[0..32])
        let c = cipher("00112233445566778899aabbccddeeff");
        let nonce: Vec<u8> = (0u8..24).collect();
        let ct = c.encrypt(Nonce::from_slice(&nonce), b"hello".as_ref()).unwrap();
        let hex: String = ct.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "c7113aa81d670128fee9170db6af90a8f5918f5e28");
    }

    #[test]
    fn seal_open_and_wrong_key() {
        let c = cipher("aaaa");
        let s = seal(&c, b"{\"p\":\"/api/deck\"}");
        assert!(!s.contains("api"));
        assert_eq!(open(&c, &s).unwrap().1, b"{\"p\":\"/api/deck\"}");
        assert!(open(&cipher("bbbb"), &s).is_none(), "다른 열쇠로는 못 연다");
        let mut bytes = STANDARD.decode(&s).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert!(open(&c, &STANDARD.encode(bytes)).is_none(), "한 글자라도 바뀌면 거절");
    }

    #[test]
    fn replay_and_clock() {
        let now = now_secs();
        let n = [7u8; 24];
        assert!(check_fresh(n, now).is_ok());
        assert!(check_fresh(n, now).is_err(), "같은 요청 두 번은 안 됨");
        assert!(check_fresh([8u8; 24], now - MAX_SKEW - 5).is_err(), "오래된 요청");
    }

    #[test]
    fn frames_roundtrip_and_tamper() {
        let c = cipher("k");
        let head = [3u8; 24];
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let body = frames(&c, head, &data, 100_000);
        let mut out = Vec::new();
        FrameReader::new(body.as_slice(), &c, head, data.len() as u64).read_to_end(&mut out).unwrap();
        assert_eq!(out, data);
        // 잘린 파일
        let cut = &body[..body.len() - 10];
        assert!(FrameReader::new(cut, &c, head, data.len() as u64).read_to_end(&mut Vec::new()).is_err());
        // 다른 머리(다른 업로드)의 조각
        assert!(FrameReader::new(body.as_slice(), &c, [4u8; 24], data.len() as u64).read_to_end(&mut Vec::new()).is_err());
    }
}
