// Rust의 config.rs 구조와 1:1로 맞춘 타입
export type Action =
  | { type: "launch"; target: string; args: string }
  | { type: "keys"; keys: string }
  | { type: "url"; url: string }
  | { type: "text"; text: string }
  | { type: "script"; command: string }
  | { type: "delay"; ms: number }
  | { type: "multi"; steps: Action[] };

export type ActionType = Action["type"];

/** 버튼 하나에 바로 지정할 수 있는 동작 (기다리기는 단독으로 의미가 없음) */
export const BUTTON_ACTION_TYPES: ActionType[] = ["launch", "keys", "url", "text", "script", "multi"];
/** 멀티 액션의 단계로 넣을 수 있는 동작 */
export const STEP_ACTION_TYPES: ActionType[] = ["launch", "keys", "url", "text", "script", "delay"];

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
  delay: "기다리기",
  multi: "멀티 액션 (여러 동작 순서대로)",
};

export const ACTION_ICONS: Record<ActionType, string> = {
  launch: "🚀",
  keys: "⌨️",
  url: "🌐",
  text: "📝",
  script: "💻",
  delay: "⏱️",
  multi: "🔗",
};

/** 단계 목록에 보여줄 한 줄 요약 */
export function summarizeAction(a: Action): string {
  switch (a.type) {
    case "launch":
      return a.target || "(경로 없음)";
    case "keys":
      return a.keys || "(키 없음)";
    case "url":
      return a.url;
    case "text":
      return a.text ? `"${a.text.slice(0, 20)}${a.text.length > 20 ? "…" : ""}"` : "(내용 없음)";
    case "script":
      return a.command.slice(0, 30) || "(명령 없음)";
    case "delay":
      return `${(a.ms / 1000).toFixed(a.ms % 1000 ? 1 : 0)}초`;
    case "multi":
      return `${a.steps.length}단계`;
  }
}

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
    case "delay":
      return { type, ms: 500 };
    case "multi":
      return { type, steps: [] };
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
