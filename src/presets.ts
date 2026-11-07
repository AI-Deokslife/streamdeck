// 프로그램별 명령 프리셋. 고르면 "프로그램 연결 폴더"가 버튼과 함께 만들어진다.
// 단축키 형식: "ctrl+s", 공백으로 구분하면 순서대로 (한글의 겹글쇠 Ctrl+N,T → "ctrl+n t")
import { DeckButton } from "./types";

type Item = [label: string, icon: string, keys: string];

export interface AppPreset {
  id: string;
  name: string;
  icon: string;
  color: string;
  /** 연결할 실행 파일 이름 */
  apps: string[];
  items: Item[];
  /** 하위 폴더로 묶을 명령들 */
  subfolders?: { name: string; icon: string; items: Item[] }[];
}

export const APP_PRESETS: AppPreset[] = [
  {
    id: "hwp",
    name: "한글",
    icon: "한",
    color: "#2b6cb0",
    apps: ["Hwp.exe"],
    // 출처: 한컴 도움말 단축키 일람 (한컴오피스 기본 단축키)
    items: [
      ["새 문서", "📄", "alt+n"],
      ["불러오기", "📂", "alt+o"],
      ["저장", "💾", "alt+s"],
      ["다른 이름으로", "🗂️", "alt+v"],
      ["인쇄", "🖨️", "alt+p"],
      ["되돌리기", "↩️", "ctrl+z"],
      ["찾기", "🔍", "ctrl+q f"],
      ["찾아 바꾸기", "🔁", "ctrl+h"],
      ["쪽 나누기", "📃", "ctrl+enter"],
      ["표 만들기", "▦", "ctrl+n t"],
      ["그림 넣기", "🖼️", "ctrl+n i"],
      ["쪽 번호", "#️⃣", "ctrl+n p"],
      ["각주", "📝", "ctrl+n n"],
      ["편집 용지", "📐", "f7"],
      ["맞춤법", "✔️", "f8"],
    ],
    subfolders: [
      {
        name: "서식",
        icon: "🔤",
        items: [
          ["글자 모양", "🔤", "alt+l"],
          ["문단 모양", "¶", "alt+t"],
          ["스타일", "🎨", "f6"],
          ["진하게", "𝐁", "ctrl+b"],
          ["기울임", "𝐼", "ctrl+i"],
          ["밑줄", "U̲", "ctrl+u"],
          ["왼쪽 정렬", "⬅️", "ctrl+shift+l"],
          ["가운데 정렬", "↔️", "ctrl+shift+c"],
          ["오른쪽 정렬", "➡️", "ctrl+shift+r"],
          ["양쪽 정렬", "☰", "ctrl+shift+m"],
          ["글자 크게", "🔠", "ctrl+]"],
          ["글자 작게", "🔡", "ctrl+["],
        ],
      },
    ],
  },
  {
    id: "word",
    name: "Word",
    icon: "W",
    color: "#2b579a",
    apps: ["WINWORD.EXE"],
    items: [
      ["새 문서", "📄", "ctrl+n"],
      ["열기", "📂", "ctrl+o"],
      ["저장", "💾", "ctrl+s"],
      ["다른 이름으로", "🗂️", "f12"],
      ["인쇄", "🖨️", "ctrl+p"],
      ["실행 취소", "↩️", "ctrl+z"],
      ["다시 실행", "↪️", "ctrl+y"],
      ["찾기", "🔍", "ctrl+f"],
      ["바꾸기", "🔁", "ctrl+h"],
      ["페이지 나누기", "📃", "ctrl+enter"],
      ["글꼴", "🔤", "ctrl+d"],
      ["진하게", "𝐁", "ctrl+b"],
      ["가운데 정렬", "↔️", "ctrl+e"],
      ["맞춤법", "✔️", "f7"],
    ],
  },
  {
    id: "excel",
    name: "Excel",
    icon: "X",
    color: "#217346",
    apps: ["EXCEL.EXE"],
    items: [
      ["저장", "💾", "ctrl+s"],
      ["셀 서식", "🔢", "ctrl+1"],
      ["자동 합계", "∑", "alt+equal"],
      ["필터", "🔽", "ctrl+shift+l"],
      ["오늘 날짜", "📅", "ctrl+semicolon"],
      ["현재 시간", "⏰", "ctrl+shift+semicolon"],
      ["행 삽입", "➕", "ctrl+shift+equal"],
      ["삭제", "➖", "ctrl+minus"],
      ["이전 시트", "◀️", "ctrl+pageup"],
      ["다음 시트", "▶️", "ctrl+pagedown"],
      ["찾기", "🔍", "ctrl+f"],
      ["실행 취소", "↩️", "ctrl+z"],
    ],
  },
  {
    id: "ppt",
    name: "PowerPoint",
    icon: "P",
    color: "#c43e1c",
    apps: ["POWERPNT.EXE"],
    items: [
      ["새 슬라이드", "➕", "ctrl+m"],
      ["슬라이드 복제", "📑", "ctrl+d"],
      ["처음부터 쇼", "▶️", "f5"],
      ["현재부터 쇼", "⏯️", "shift+f5"],
      ["그룹", "🔗", "ctrl+g"],
      ["그룹 해제", "⛓️", "ctrl+shift+g"],
      ["저장", "💾", "ctrl+s"],
      ["실행 취소", "↩️", "ctrl+z"],
    ],
  },
  {
    id: "browser",
    name: "웹 브라우저",
    icon: "🌐",
    color: "#2c7a7b",
    apps: ["chrome.exe", "msedge.exe", "whale.exe", "firefox.exe"],
    items: [
      ["새 탭", "➕", "ctrl+t"],
      ["탭 닫기", "✖️", "ctrl+w"],
      ["닫은 탭 열기", "♻️", "ctrl+shift+t"],
      ["새로고침", "🔄", "f5"],
      ["시크릿 창", "🕶️", "ctrl+shift+n"],
      ["주소창", "🔗", "ctrl+l"],
      ["다운로드", "⬇️", "ctrl+j"],
      ["방문 기록", "🕘", "ctrl+h"],
      ["다음 탭", "▶️", "ctrl+tab"],
      ["이전 탭", "◀️", "ctrl+shift+tab"],
      ["확대", "🔍", "ctrl+equal"],
      ["원래 크기", "1️⃣", "ctrl+0"],
    ],
  },
  {
    id: "vscode",
    name: "VS Code",
    icon: "💻",
    color: "#0078d4",
    apps: ["Code.exe"],
    items: [
      ["명령 팔레트", "🎛️", "ctrl+shift+p"],
      ["파일 찾기", "📄", "ctrl+p"],
      ["저장", "💾", "ctrl+s"],
      ["터미널", "⌨️", "ctrl+backquote"],
      ["사이드바", "📚", "ctrl+b"],
      ["전체 검색", "🔍", "ctrl+shift+f"],
      ["코드 정리", "✨", "shift+alt+f"],
      ["주석", "💬", "ctrl+slash"],
      ["정의로 이동", "🎯", "f12"],
      ["줄 복제", "📑", "shift+alt+down"],
    ],
  },
];

function keyButton([label, icon, keys]: Item, color: string): DeckButton {
  return {
    id: crypto.randomUUID(),
    label,
    icon,
    color,
    hotkey: "",
    action: { type: "keys", keys, app: "" },
  };
}

/** 프리셋 → 프로그램 연결 폴더 버튼 */
export function presetFolder(p: AppPreset, launch = "", image = ""): DeckButton {
  const children = p.items.map((it) => keyButton(it, p.color));
  for (const sub of p.subfolders ?? []) {
    children.push({
      id: crypto.randomUUID(),
      label: sub.name,
      icon: sub.icon,
      color: "#4a5568",
      hotkey: "",
      action: { type: "folder", apps: [], launch: "", buttons: sub.items.map((it) => keyButton(it, "#4a5568")) },
    });
  }
  return {
    id: crypto.randomUUID(),
    label: p.name,
    icon: p.icon,
    // 프로그램 아이콘이 있으면 이미지로 (프리셋 추가 시 설치된 프로그램에서 꺼내 옴)
    image,
    color: image ? "#1f2430" : p.color,
    hotkey: "",
    action: { type: "folder", apps: [...p.apps], launch, buttons: children },
  };
}
