import { useRef, useState } from "react";

const MODIFIER_CODES = new Set([
  "ControlLeft", "ControlRight", "AltLeft", "AltRight",
  "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight",
]);

/** 보낼 키용 이름 (Rust actions::parse_key 가 이해하는 이름) */
const SEND_NAMES: Record<string, string> = {
  Escape: "esc",
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  NumpadEnter: "enter",
  AudioVolumeUp: "volumeup",
  AudioVolumeDown: "volumedown",
  AudioVolumeMute: "mute",
  MediaPlayPause: "playpause",
  MediaTrackNext: "next",
  MediaTrackPrevious: "prev",
  PrintScreen: "printscreen",
};

/** 수정키 없이 단독으로 전역 단축키가 돼도 괜찮은 키 (글자 키는 타이핑을 막으므로 안 됨) */
function isSafeAlone(code: string) {
  return /^F\d+$/.test(code) || code.startsWith("Audio") || code.startsWith("Media") || code === "Pause";
}

/** 키보드 이벤트 → 문자열.
 *  mode="global": 전역 단축키용 (예: "Ctrl+Alt+KeyK")
 *  mode="send":   보낼 키용   (예: "ctrl+alt+k") */
function comboFromEvent(e: React.KeyboardEvent, mode: "global" | "send"): string | null {
  if (MODIFIER_CODES.has(e.code) || !e.code) return null; // 수정키만 눌린 상태
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push(mode === "global" ? "Super" : "Win");

  if (mode === "global") return [...mods, e.code].join("+");

  const name = SEND_NAMES[e.code] ?? e.code.replace(/^Key/, "").replace(/^Digit/, "");
  return [...mods, name].join("+").toLowerCase();
}

const LABELS: Record<string, string> = {
  ctrl: "Ctrl", control: "Ctrl", alt: "Alt", shift: "Shift", win: "Win", super: "Win", meta: "Win",
  esc: "Esc", escape: "Esc", enter: "Enter", tab: "Tab", space: "Space", backspace: "⌫ Backspace",
  delete: "Del", insert: "Ins", home: "Home", end: "End", pageup: "PgUp", pagedown: "PgDn",
  up: "↑", down: "↓", left: "←", right: "→", arrowup: "↑", arrowdown: "↓", arrowleft: "←", arrowright: "→",
  volumeup: "🔊 볼륨+", volumedown: "🔉 볼륨-", mute: "🔇 음소거", playpause: "⏯ 재생",
  next: "⏭ 다음", prev: "⏮ 이전", printscreen: "PrtSc",
  minus: "-", equal: "=", comma: ",", period: ".", slash: "/", semicolon: ";", quote: "'",
  backquote: "`", bracketleft: "[", bracketright: "]", backslash: "\\",
};

/** 키 하나의 표시 이름: "KeyK"/"k" → "K", "ctrl" → "Ctrl" */
function keyLabel(token: string): string {
  const t = token.trim();
  const low = t.toLowerCase();
  if (LABELS[low]) return LABELS[low];
  const m = /^(?:key|digit)(.)$/i.exec(t);
  if (m) return m[1].toUpperCase();
  if (/^numpad\d$/i.test(t)) return "Num " + t.slice(-1);
  if (/^f\d+$/i.test(t)) return t.toUpperCase();
  if (t.length === 1) return t.toUpperCase();
  return t.replace(/^Audio|^Media/, "");
}

/** "ctrl+n t" → [["Ctrl","N"],["T"]] */
function parseCombos(s: string): string[][] {
  return s
    .split(/\s+/)
    .filter(Boolean)
    .map((combo) => combo.split("+").filter(Boolean).map(keyLabel));
}

/** 표시용: "Ctrl+Alt+KeyK" → "Ctrl + Alt + K" */
export function prettyHotkey(s: string): string {
  return parseCombos(s)
    .map((c) => c.join(" + "))
    .join(" , ");
}

interface Props {
  value: string;
  onChange: (v: string) => void;
  mode: "global" | "send";
  placeholder?: string;
}

/** 칸을 클릭하고 키를 누르면 그대로 기록된다.
 *  - 보낼 키(send): 이어서 누르면 순서대로 추가 (한글 겹글쇠 Ctrl+N, T 같은 경우). ⌫ 는 마지막 것 지우기
 *  - 전역 단축키(global): 누른 조합으로 바뀜
 *  볼륨 같은 특수 이름은 [직접 입력]으로 쓸 수 있다. */
export default function HotkeyInput({ value, onChange, mode, placeholder }: Props) {
  const [focused, setFocused] = useState(false);
  const [typing, setTyping] = useState(false);
  const [warn, setWarn] = useState("");
  /** 이번에 칸을 누른 뒤 기록한 것이 있는지 (첫 입력은 기존 값을 바꾸고, 이후는 이어 붙임) */
  const recordedThisFocus = useRef(false);

  function onKeyDown(e: React.KeyboardEvent) {
    if (e.code === "Tab" && !e.ctrlKey && !e.altKey && !e.metaKey) {
      return; // Tab / Shift+Tab 은 다음·이전 칸으로 이동 (Tab 자체를 보내려면 ✎ 직접 입력)
    }
    e.preventDefault();
    e.stopPropagation();
    const noMods = !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey;

    // ⌫ 단독: 마지막 조합 지우기
    if (e.code === "Backspace" && noMods) {
      const parts = value.split(/\s+/).filter(Boolean);
      parts.pop();
      onChange(mode === "send" ? parts.join(" ") : "");
      setWarn("");
      return;
    }
    const combo = comboFromEvent(e, mode);
    if (!combo) return;

    if (mode === "global") {
      if (noMods && !isSafeAlone(e.code)) {
        setWarn("전역 단축키는 Ctrl·Alt·Shift·Win 중 하나와 함께 눌러 주세요 (글자만 쓰면 타이핑이 막혀요)");
        return;
      }
      setWarn("");
      onChange(combo);
      return;
    }
    setWarn("");
    onChange(recordedThisFocus.current && value ? `${value} ${combo}` : combo);
    recordedThisFocus.current = true;
  }

  if (typing) {
    return (
      <div className="hotkey-input">
        <input
          autoFocus
          value={value}
          placeholder={mode === "send" ? "예: ctrl+shift+esc, volumeup, ctrl+n t" : "예: Ctrl+Alt+KeyK"}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && setTyping(false)}
        />
        <button type="button" className="small" onClick={() => setTyping(false)}>
          완료
        </button>
      </div>
    );
  }

  const combos = parseCombos(value);
  return (
    <div className="hotkey-wrap">
      <div className="hotkey-input">
        <div
          className={"keycap-box" + (focused ? " recording" : "")}
          tabIndex={0}
          role="textbox"
          aria-label="단축키 입력 — 키를 누르세요"
          onFocus={() => {
            setFocused(true);
            recordedThisFocus.current = false;
          }}
          onBlur={() => {
            setFocused(false);
            setWarn("");
          }}
          onKeyDown={onKeyDown}
        >
          {combos.length === 0 && (
            <span className="keycap-placeholder">{focused ? "키를 누르세요…" : placeholder ?? "클릭하고 키를 누르세요"}</span>
          )}
          {combos.map((keys, i) => (
            <span key={i} className="keycap-combo">
              {i > 0 && <span className="keycap-then">→</span>}
              {keys.map((k, j) => (
                <span key={j}>
                  {j > 0 && <span className="keycap-plus">+</span>}
                  <kbd>{k}</kbd>
                </span>
              ))}
            </span>
          ))}
          {focused && combos.length > 0 && <span className="keycap-caret" />}
        </div>
        <button type="button" className="small ghost" title="글자로 직접 입력" onClick={() => setTyping(true)}>
          ✎
        </button>
        {value && (
          <button
            type="button"
            className="small ghost"
            title="지우기"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => onChange("")}
          >
            ✕
          </button>
        )}
      </div>
      {focused && !warn && (
        <span className="keycap-hint">
          {mode === "send"
            ? "이어서 누르면 순서대로 추가돼요 (예: Ctrl+N 다음 T) · ⌫ 마지막 지우기"
            : "누른 조합으로 바로 바뀌어요 · ⌫ 지우기"}
        </span>
      )}
      {warn && <span className="keycap-hint warn">{warn}</span>}
    </div>
  );
}
