// Rust의 config.rs 구조와 1:1로 맞춘 타입
export type Action =
  | { type: "launch"; target: string; args: string }
  | { type: "keys"; keys: string }
  | { type: "url"; url: string }
  | { type: "text"; text: string }
  | { type: "script"; command: string };

export type ActionType = Action["type"];

export interface DeckButton {
  id: string;
  label: string;
  icon: string;
  color: string;
  hotkey: string;
  action: Action;
}

export interface Config {
  toggleHotkey: string;
  columns: number;
  buttons: DeckButton[];
}

export const ACTION_LABELS: Record<ActionType, string> = {
  launch: "프로그램 / 파일 실행",
  keys: "단축키 보내기",
  url: "웹사이트 열기",
  text: "텍스트 입력",
  script: "PowerShell 명령",
};

export function emptyAction(type: ActionType): Action {
  switch (type) {
    case "launch":
      return { type, target: "", args: "" };
    case "keys":
      return { type, keys: "" };
    case "url":
      return { type, url: "https://" };
    case "text":
      return { type, text: "" };
    case "script":
      return { type, command: "" };
  }
}

export function newButton(): DeckButton {
  return {
    id: crypto.randomUUID(),
    label: "새 버튼",
    icon: "⭐",
    color: "#2d3748",
    hotkey: "",
    action: emptyAction("launch"),
  };
}
