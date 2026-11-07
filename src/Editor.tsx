import HotkeyInput from "./HotkeyInput";
import { ACTION_LABELS, Action, ActionType, Config, DeckButton, emptyAction } from "./types";

const COLORS = ["#2d3748", "#2b6cb0", "#2f855a", "#c05621", "#9b2c2c", "#6b46c1", "#2c7a7b", "#b7791f", "#d53f8c"];

const KEY_PRESETS: [string, string][] = [
  ["볼륨 +", "volumeup"],
  ["볼륨 -", "volumedown"],
  ["음소거", "mute"],
  ["재생/정지", "playpause"],
  ["다음 곡", "next"],
  ["이전 곡", "prev"],
  ["복사", "ctrl+c"],
  ["붙여넣기", "ctrl+v"],
  ["화면 캡처", "win+shift+s"],
  ["바탕화면", "win+d"],
];

// ---------------- 선택된 버튼 편집 ----------------

interface ButtonEditorProps {
  button: DeckButton;
  onChange: (b: DeckButton) => void;
  onDelete: () => void;
  onTest: () => void;
}

export function ButtonEditor({ button, onChange, onDelete, onTest }: ButtonEditorProps) {
  const set = (patch: Partial<DeckButton>) => onChange({ ...button, ...patch });
  const setAction = (patch: Partial<Action>) => set({ action: { ...button.action, ...patch } as Action });
  const a = button.action;

  return (
    <div className="editor">
      <div className="row">
        <label className="grow">
          이름
          <input value={button.label} onChange={(e) => set({ label: e.target.value })} />
        </label>
        <label style={{ width: 70 }}>
          아이콘
          <input
            value={button.icon}
            onChange={(e) => set({ icon: e.target.value })}
            placeholder="이모지"
            className="center"
          />
        </label>
      </div>

      <label>색상</label>
      <div className="colors">
        {COLORS.map((c) => (
          <button
            key={c}
            type="button"
            className={"swatch" + (button.color === c ? " active" : "")}
            style={{ background: c }}
            onClick={() => set({ color: c })}
          />
        ))}
        <input type="color" value={button.color} onChange={(e) => set({ color: e.target.value })} />
      </div>

      <label>
        동작
        <select
          value={a.type}
          onChange={(e) => set({ action: emptyAction(e.target.value as ActionType) })}
        >
          {Object.entries(ACTION_LABELS).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </select>
      </label>

      {a.type === "launch" && (
        <>
          <label>
            실행할 프로그램 / 파일 / 폴더 경로
            <input
              value={a.target}
              placeholder="예: notepad.exe 또는 C:\Program Files\...\app.exe"
              onChange={(e) => setAction({ target: e.target.value })}
            />
          </label>
          <label>
            인자 (선택)
            <input value={a.args} onChange={(e) => setAction({ args: e.target.value })} />
          </label>
        </>
      )}

      {a.type === "keys" && (
        <>
          <label>보낼 키 (공백으로 구분하면 순서대로 여러 번)</label>
          <HotkeyInput mode="send" value={a.keys} onChange={(v) => setAction({ keys: v })} placeholder="예: ctrl+shift+esc" />
          <div className="chips">
            {KEY_PRESETS.map(([name, keys]) => (
              <button key={keys} type="button" className="chip" onClick={() => setAction({ keys })}>
                {name}
              </button>
            ))}
          </div>
        </>
      )}

      {a.type === "url" && (
        <label>
          주소
          <input value={a.url} onChange={(e) => setAction({ url: e.target.value })} />
        </label>
      )}

      {a.type === "text" && (
        <label>
          입력할 텍스트
          <textarea rows={3} value={a.text} onChange={(e) => setAction({ text: e.target.value })} />
        </label>
      )}

      {a.type === "script" && (
        <label>
          PowerShell 명령
          <textarea
            rows={3}
            value={a.command}
            placeholder='예: Start-Process "https://youtube.com"'
            onChange={(e) => setAction({ command: e.target.value })}
          />
        </label>
      )}

      <label>이 버튼의 전역 단축키 (선택)</label>
      <HotkeyInput mode="global" value={button.hotkey} onChange={(v) => set({ hotkey: v })} placeholder="없음" />

      <div className="row end">
        <button type="button" className="danger" onClick={onDelete}>
          삭제
        </button>
        <button type="button" onClick={onTest} title="지금 설정으로 한 번 실행해 봅니다">
          테스트 실행
        </button>
      </div>
    </div>
  );
}

// ---------------- 전체 설정 ----------------

interface SettingsProps {
  config: Config;
  onChange: (c: Config) => void;
}

export function SettingsEditor({ config, onChange }: SettingsProps) {
  return (
    <div className="editor">
      <p className="hint">버튼을 클릭하면 편집하고, 끌어서 순서를 바꿀 수 있어요.</p>
      <label>창 보이기 / 숨기기 단축키</label>
      <HotkeyInput
        mode="global"
        value={config.toggleHotkey}
        onChange={(v) => onChange({ ...config, toggleHotkey: v })}
      />
      <label>
        한 줄에 버튼 수: {config.columns}
        <input
          type="range"
          min={2}
          max={8}
          value={config.columns}
          onChange={(e) => onChange({ ...config, columns: Number(e.target.value) })}
        />
      </label>
    </div>
  );
}
