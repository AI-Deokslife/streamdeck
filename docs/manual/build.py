"""사용설명서 PDF 만들기.

1) manual.html 의 각 소단원 제목에 보이지 않는 표식을 넣어 PDF 로 한 번 출력하고
2) 표식이 몇 쪽에 있는지 찾아 차례의 쪽 번호를 채운 뒤
3) 최종 PDF 를 docs/QuickDeck_사용설명서.pdf 로 출력한다.

필요: Microsoft Edge, PyMuPDF(fitz)
실행: python docs/manual/build.py [--preview]   (--preview: 쪽별 PNG 도 저장)
"""
import re
import subprocess
import sys
import tempfile
from pathlib import Path

import fitz  # PyMuPDF

HERE = Path(__file__).parent
SRC = HERE / "manual.html"
OUT = HERE.parent / "QuickDeck_사용설명서.pdf"
EDGE = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"


def render(html_path: Path, pdf_path: Path) -> None:
    # 실행할 때마다 새 프로필 (다른 Edge 가 같은 프로필을 잡고 있으면 출력이 조용히 실패한다)
    profile = tempfile.mkdtemp(prefix="qd-edge-")
    subprocess.run(
        [
            EDGE, "--headless=new", "--disable-gpu", f"--user-data-dir={profile}",
            "--no-pdf-header-footer", "--virtual-time-budget=8000",
            f"--print-to-pdf={pdf_path}", html_path.resolve().as_uri(),
        ],
        check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    if not pdf_path.exists():
        sys.exit(f"PDF 출력 실패: {pdf_path}")


def main() -> None:
    html = SRC.read_text(encoding="utf-8")
    titles = list(dict.fromkeys(re.findall(r'data-find="([^"]+)"', html)))

    # 1) 소단원 제목마다 표식 ⟦n⟧ 넣기 (흰색 아주 작은 글씨 → 인쇄물에는 안 보임)
    marked = html.replace("</style>", ".mk{font-size:2px;color:#fff;}</style>", 1)
    for i, t in enumerate(titles):
        pattern = f"</span>{t}</h2>"
        if pattern not in marked:
            sys.exit(f"제목을 찾지 못함: {t}")
        marked = marked.replace(pattern, f'</span>{t}<span class="mk">QDMK{i:02d}</span></h2>', 1)

    with tempfile.TemporaryDirectory() as tmp:
        tmp_html = HERE / "_build_pass1.html"  # 상대 경로 그림을 위해 같은 폴더에 둔다
        tmp_pdf = Path(tmp) / "pass1.pdf"
        tmp_html.write_text(marked, encoding="utf-8")
        try:
            render(tmp_html, tmp_pdf)
        finally:
            tmp_html.unlink(missing_ok=True)

        # 2) 표식이 있는 쪽 찾기 (쪽 번호 = 0부터 센 쪽 + 1, 표지가 1쪽)
        doc = fitz.open(tmp_pdf)
        pages = {}
        for pno in range(doc.page_count):
            text = doc[pno].get_text()
            for m in re.findall(r"QDMK(\d\d)", text):
                pages.setdefault(int(m), pno + 1)
        doc.close()

    missing = [titles[i] for i in range(len(titles)) if i not in pages]
    if missing:
        sys.exit(f"쪽을 찾지 못한 제목: {missing}")

    # 3) 차례 채우고 최종 출력 (표식은 넣지 않음 — 쪽 배치는 같다)
    final = html
    for i, t in enumerate(titles):
        final = final.replace(f'data-find="{t}"></span>', f'data-find="{t}">{pages[i]}</span>')
    final_html = HERE / "_build_final.html"
    final_html.write_text(final, encoding="utf-8")
    try:
        render(final_html, OUT)
    finally:
        final_html.unlink(missing_ok=True)

    doc = fitz.open(OUT)
    print(f"완성: {OUT} ({doc.page_count}쪽)")
    for i, t in enumerate(titles):
        print(f"  {pages[i]:>3}쪽  {t}")
    if "--preview" in sys.argv:
        prev = HERE / "_preview"
        prev.mkdir(exist_ok=True)
        for p in prev.glob("*.png"):
            p.unlink()
        for pno in range(doc.page_count):
            doc[pno].get_pixmap(dpi=70).save(prev / f"page{pno + 1:02d}.png")
        print(f"미리보기: {prev}")
    doc.close()


if __name__ == "__main__":
    main()
