# 사용설명서 스크린샷 만들기. 먼저 프로젝트 폴더에서 `npx vite --port 1420` 을 실행해 둔다.
$ErrorActionPreference = "Continue"
$edge = "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
$out = Join-Path $PSScriptRoot "assets"
$prof = Join-Path $env:TEMP "quickdeck-manual-edge"
$shots = @(
  @{ n = "home-dark";    v = "home";        t = "dark";  h = 330 },
  @{ n = "home-light";   v = "home";        t = "light"; h = 330 },
  @{ n = "folder";       v = "folder";      t = "dark";  h = 330 },
  @{ n = "auto";         v = "auto";        t = "dark";  h = 330 },
  @{ n = "edit";         v = "edit";        t = "dark";  h = 720 },
  @{ n = "edit-icon";    v = "edit-icon";   t = "dark";  h = 720 },
  @{ n = "edit-keys";    v = "edit-keys";   t = "dark";  h = 720 },
  @{ n = "edit-folder";  v = "edit-folder"; t = "dark";  h = 720 },
  @{ n = "edit-multi";   v = "edit-multi";  t = "dark";  h = 720 },
  @{ n = "settings";     v = "settings";    t = "light"; h = 720 },
  @{ n = "settings-2";   v = "settings-2";  t = "light"; h = 720 },
  @{ n = "settings-remote"; v = "settings-remote"; t = "light"; h = 720 },
  @{ n = "settings-usb"; v = "settings-usb"; t = "light"; h = 720 },
  @{ n = "settings-usb-wait"; v = "settings-usb-wait"; t = "light"; h = 720 },
  @{ n = "settings-look"; v = "settings-look"; t = "light"; h = 720 },
  @{ n = "widgets";      v = "widgets";     t = "dark";  h = 330 },
  @{ n = "widgets-edit"; v = "widgets-edit"; t = "dark"; h = 720 },
  @{ n = "home-kakao";   v = "home";        t = "kakao"; h = 330 },
  @{ n = "home-toss";    v = "home";        t = "toss";  h = 330 },
  @{ n = "home-baemin";  v = "home";        t = "baemin"; h = 330 },
  @{ n = "home-line";    v = "home";        t = "line";  h = 330 },
  @{ n = "opacity";      v = "home";        t = "dark";  h = 330; q = "&op=60&bg=check" }
)
foreach ($s in $shots) {
  $file = Join-Path $out "shot-$($s.n).png"
  if (Test-Path $file) { Remove-Item $file }
  & $edge --headless=new --disable-gpu --hide-scrollbars --user-data-dir="$prof-$($s.n)" --force-device-scale-factor=2 `
    --window-size=600,$($s.h) --virtual-time-budget=6000 --screenshot="$file" `
    "http://localhost:1420/docs/manual/shots.html?view=$($s.v)&theme=$($s.t)&w=420&h=$($s.h)$($s.q)" 2>$null | Out-Null
  if (Test-Path $file) { "ok   $($s.n)" } else { "FAIL $($s.n)" }
}
