import { useState } from "react";

const MODIFIER_CODES = new Set([
  "ControlLeft", "ControlRight", "AltLeft", "AltRight",
  "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight",
]);

/** 키보드 이벤트 → 문자열.
 *  mode="global": 전역 단축키용 (예: "Ctrl+Alt+KeyK")
 *  mode="send":   보낼 키용   (예: "ctrl+alt+k") */
function comboFromEvent(e: React.KeyboardEvent, mode: "global" | "send"): string | null {
  if (MODIFIER_CODES.has(e.code)) return null; // 수정키만 눌린 상태
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push(mode === "global" ? "Super" : "Win");

  if (mode === "global") return [...mods, e.code].join("+");

  const friendly = e.code
    .replace(/^Key/, "")
    .replace(/^Digit/, "")
    .replace(/^Arrow/, "")
    .replace(/^Escape$/, "esc");
  return [...mods, friendly].join("+").toLowerCase();
}

/** 표시용: "Ctrl+Alt+KeyK" → "Ctrl + Alt + K" */
export function prettyHotkey(s: string): string {
  if (!s) return "";
  return s
    .split("+")
    .map((p) => p.replace(/^Key(.)$/, "$1").replace(/^Digit/, ""))
    .join(" + ");
}

interface Props {
  value: string;
  onChange: (v: string) => void;
  mode: "global" | "send";
  placeholder?: string;
}

/** 직접 입력도 되고, [녹화]를 누르면 다음 키 조합을 그대로 기록한다. */
export default function HotkeyInput({ value, onChange, mode, placeholder }: Props) {
  const [recording, setRecording] = useState(false);

  return (
    <div className="hotkey-input">
      <input
        value={recording ? "키 조합을 누르세요… (Esc 취소)" : value}
        readOnly={recording}
        placeholder={placeholder}
        className={recording ? "recording" : ""}
        onChange={(e) => onChange(e.target.value)}
        onBlur={() => setRecording(false)}
        onKeyDown={(e) => {
          if (!recording) return;
          e.preventDefault();
          if (e.code === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey) {
            setRecording(false);
            return;
          }
          const combo = comboFromEvent(e, mode);
          if (combo) {
            onChange(combo);
            setRecording(false);
          }
        }}
        ref={(el) => {
          if (recording) el?.focus();
        }}
      />
      <button type="button" className="small" onClick={() => setRecording((r) => !r)}>
        {recording ? "취소" : "녹화"}
      </button>
      {value && !recording && (
        <button type="button" className="small ghost" onClick={() => onChange("")} title="지우기">
          ✕
        </button>
      )}
    </div>
  );
}
