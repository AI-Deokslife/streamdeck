//! 소리와 음악: 시스템 볼륨(Core Audio)과 지금 재생 중인 음악(Windows 미디어 컨트롤).
//! 유튜브·멜론·스포티파이 등 Windows 미디어 키를 지원하는 프로그램의 곡 정보를 읽는다.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use std::sync::Mutex;
use windows::{
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    },
    Storage::Streams::DataReader,
    Win32::{
        Media::Audio::{eConsole, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator},
        System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED},
    },
};

// ---------------- 볼륨 ----------------

fn endpoint() -> Result<IAudioEndpointVolume, String> {
    unsafe {
        // 이미 초기화된 스레드면 실패해도 괜찮다
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let en: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER).map_err(|e| format!("오디오 장치를 찾지 못했어요: {e}"))?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).map_err(|e| format!("스피커를 찾지 못했어요: {e}"))?;
        dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).map_err(|e| format!("볼륨을 읽지 못했어요: {e}"))
    }
}

/// (볼륨 0~100, 음소거 여부)
pub fn volume() -> Result<(u32, bool), String> {
    let ep = endpoint()?;
    unsafe {
        let v = ep.GetMasterVolumeLevelScalar().map_err(|e| e.to_string())?;
        let m = ep.GetMute().map_err(|e| e.to_string())?.as_bool();
        Ok(((v * 100.0).round() as u32, m))
    }
}

pub fn set_volume(percent: i32) -> Result<(), String> {
    let ep = endpoint()?;
    let v = percent.clamp(0, 100) as f32 / 100.0;
    unsafe {
        ep.SetMasterVolumeLevelScalar(v, std::ptr::null()).map_err(|e| e.to_string())?;
        // 소리를 올리면 음소거도 풀어 준다
        if v > 0.0 {
            let _ = ep.SetMute(false, std::ptr::null());
        }
    }
    Ok(())
}

pub fn toggle_mute() -> Result<(), String> {
    let ep = endpoint()?;
    unsafe {
        let m = ep.GetMute().map_err(|e| e.to_string())?.as_bool();
        ep.SetMute(!m, std::ptr::null()).map_err(|e| e.to_string())
    }
}

// ---------------- 지금 재생 중인 음악 ----------------

#[derive(Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    /// 재생 중인 프로그램 (예: Spotify.exe, chrome)
    pub source: String,
    /// 앨범 그림 data URL (없으면 빈 문자열)
    pub art: String,
}

/// 같은 곡의 앨범 그림은 다시 읽지 않는다
static ART_CACHE: Mutex<Option<(String, String)>> = Mutex::new(None);

fn session() -> Option<Session> {
    let mgr = Manager::RequestAsync().ok()?.get().ok()?;
    mgr.GetCurrentSession().ok()
}

pub fn now_playing() -> Option<NowPlaying> {
    let s = session()?;
    let props = s.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let title = props.Title().map(|t| t.to_string()).unwrap_or_default();
    let artist = props.Artist().map(|t| t.to_string()).unwrap_or_default();
    if title.is_empty() && artist.is_empty() {
        return None;
    }
    let playing = s
        .GetPlaybackInfo()
        .and_then(|i| i.PlaybackStatus())
        .map(|st| st == Status::Playing)
        .unwrap_or(false);
    let source = s.SourceAppUserModelId().map(|t| t.to_string()).unwrap_or_default();

    let key = format!("{title}\u{1}{artist}");
    let cached = ART_CACHE.lock().unwrap().as_ref().filter(|(k, _)| *k == key).map(|(_, a)| a.clone());
    let art = cached.unwrap_or_else(|| {
        let art = read_thumbnail(&props).unwrap_or_default();
        *ART_CACHE.lock().unwrap() = Some((key, art.clone()));
        art
    });
    Some(NowPlaying { title, artist, playing, source, art })
}

fn read_thumbnail(
    props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Option<String> {
    let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.get().ok()?;
    let size = stream.Size().ok()?.min(2_000_000) as u32;
    if size == 0 {
        return None;
    }
    let mime = stream.ContentType().map(|t| t.to_string()).unwrap_or_else(|_| "image/png".into());
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.LoadAsync(size).ok()?.get().ok()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf).ok()?;
    let mime = if mime.starts_with("image/") { mime } else { "image/png".into() };
    Some(format!("data:{mime};base64,{}", STANDARD.encode(&buf)))
}

/// "toggle" / "next" / "prev"
pub fn media_command(cmd: &str) -> Result<(), String> {
    let s = session().ok_or("지금 재생 중인 음악이 없어요")?;
    let op = match cmd {
        "next" => s.TrySkipNextAsync(),
        "prev" => s.TrySkipPreviousAsync(),
        _ => s.TryTogglePlayPauseAsync(),
    };
    op.and_then(|o| o.get()).map(|_| ()).map_err(|e| format!("음악 조작 실패: {e}"))
}

#[cfg(test)]
mod tests {
    /// 이 PC 의 볼륨을 읽을 수 있는지 (바꾸지는 않음)
    #[test]
    fn reads_volume() {
        match super::volume() {
            Ok((v, _)) => assert!(v <= 100),
            Err(e) => println!("오디오 장치 없음 (CI 등): {e}"),
        }
    }

    /// 재생 중인 곡이 있으면 정보를 출력 (없어도 통과)
    #[test]
    fn reads_now_playing() {
        if let Some(n) = super::now_playing() {
            println!("재생 중: {} - {} ({}) art={}B", n.title, n.artist, n.source, n.art.len());
        }
    }
}
