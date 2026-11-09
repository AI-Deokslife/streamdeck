"""shots.ps1 로 찍은 스크린샷 다듬기: 앱 너비(420px×2)만 남기고, 편집 칸 사진은 아래 시트 부분만 자른다."""
from pathlib import Path

from PIL import Image

ASSETS = Path(__file__).parent / "assets"
APP_WIDTH = 840  # 420px × 배율 2
SHEET_ONLY = {"edit-keys", "edit-folder", "edit-multi", "settings", "settings-2", "settings-remote", "settings-usb", "settings-usb-wait", "settings-look"}
SHEET_TOP = 690  # 편집 모드에서 버튼 두 줄 + 추가 칸 아래 (배율 2 기준)

for f in sorted(ASSETS.glob("shot-*.png")):
    im = Image.open(f)
    name = f.stem.removeprefix("shot-")
    top = SHEET_TOP if name in SHEET_ONLY else 0
    im.crop((0, top, min(APP_WIDTH, im.width), im.height)).save(f)
    print(f.name, im.size, "->", (APP_WIDTH, im.height - top))
