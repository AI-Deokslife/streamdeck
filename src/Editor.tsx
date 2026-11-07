import { useState } from "react";
import HotkeyInput from "./HotkeyInput";
import { FolderOption, Path, countDeep } from "./tree";
import {
  ACTION_ICONS,
  ACTION_LABELS,
  Action,
  ActionType,
  BUTTON_ACTION_TYPES,
  Config,
  DeckButton,
  STEP_ACTION_TYPES,
  emptyAction,
  summarizeAction,
} from "./types";

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

// ---------------- 동작 하나의 입력 필드 ----------------

function ActionFields({ action: a, onChange }: { action: Action; onChange: (a: Action) => void }) {
  const set = (patch: Partial<Action>) => onChange({ ...a, ...patch } as Action);

  switch (a.type) {
    case "launch":
      return (
        <>
          <label>
            실행할 프로그램 / 파일 / 폴더 경로
            <input
              value={a.target}
              placeholder="예: notepad.exe 또는 C:\Program Files\...\app.exe"
              onChange={(e) => set({ target: e.target.value })}
            />
          </label>
          <label>
            인자 (선택)
            <input value={a.args} onChange={(e) => set({ args: e.target.value })} />
          </label>
        </>
      );
    case "keys":
      return (
        <>
          <label>보낼 키 (공백으로 구분하면 순서대로 여러 번)</label>
          <HotkeyInput mode="send" value={a.keys} onChange={(v) => set({ keys: v })} placeholder="예: ctrl+shift+esc" />
          <div className="chips">
            {KEY_PRESETS.map(([name, keys]) => (
              <button key={keys} type="button" className="chip" onClick={() => set({ keys })}>
                {name}
              </button>
            ))}
          </div>
        </>
      );
    case "url":
      return (
        <label>
          주소
          <input value={a.url} onChange={(e) => set({ url: e.target.value })} />
        </label>
      );
    case "text":
      return (
        <label>
          입력할 텍스트
          <textarea rows={3} value={a.text} onChange={(e) => set({ text: e.target.value })} />
        </label>
      );
    case "script":
      return (
        <label>
          PowerShell 명령
          <textarea
            rows={3}
            value={a.command}
            placeholder='예: Start-Process "https://youtube.com"'
            onChange={(e) => set({ command: e.target.value })}
          />
        </label>
      );
    case "delay":
      return (
        <label>
          기다릴 시간: {(a.ms / 1000).toFixed(1)}초
          <div className="row">
            <input
              type="range"
              className="grow"
              min={100}
              max={10000}
              step={100}
              value={a.ms}
              onChange={(e) => set({ ms: Number(e.target.value) })}
            />
            <input
              type="number"
              style={{ width: 80 }}
              min={0}
              max={60000}
              step={100}
              value={a.ms}
              onChange={(e) => set({ ms: Math.max(0, Math.min(60000, Number(e.target.value) || 0)) })}
            />
            <span className="unit">ms</span>
          </div>
        </label>
      );
    case "multi":
      return <StepList steps={a.steps} onChange={(steps) => set({ steps })} />;
    case "folder":
      return null; // 폴더는 ButtonEditor에서 따로 표시
  }
}

function TypeSelect({
  value,
  types,
  onChange,
}: {
  value: ActionType;
  types: ActionType[];
  onChange: (t: ActionType) => void;
}) {
  return (
    <select value={value} onChange={(e) => onChange(e.target.value as ActionType)}>
      {types.map((t) => (
        <option key={t} value={t}>
          {ACTION_ICONS[t]} {ACTION_LABELS[t]}
        </option>
      ))}
    </select>
  );
}

// ---------------- 멀티 액션 단계 목록 ----------------

function StepList({ steps, onChange }: { steps: Action[]; onChange: (s: Action[]) => void }) {
  const [open, setOpen] = useState<number | null>(null);

  const update = (i: number, a: Action) => onChange(steps.map((s, j) => (j === i ? a : s)));
  const remove = (i: number) => {
    onChange(steps.filter((_, j) => j !== i));
    setOpen(null);
  };
  const move = (i: number, dir: -1 | 1) => {
    const j = i + dir;
    if (j < 0 || j >= steps.length) return;
    const list = [...steps];
    [list[i], list[j]] = [list[j], list[i]];
    onChange(list);
    if (open === i) setOpen(j);
  };
  const add = (t: ActionType) => {
    onChange([...steps, emptyAction(t)]);
    setOpen(steps.length);
  };

  return (
    <div className="steps">
      <label>단계 ({steps.length}) — 위에서부터 순서대로 실행돼요</label>
      {steps.length === 0 && <p className="hint">아래에서 단계를 추가하세요.</p>}
      {steps.map((s, i) => (
        <div key={i} className={"step" + (open === i ? " open" : "")}>
          <div className="step-head" onClick={() => setOpen(open === i ? null : i)}>
            <span className="step-no">{i + 1}</span>
            <span className="step-icon">{ACTION_ICONS[s.type]}</span>
            <span className="step-summary">
              <b>{ACTION_LABELS[s.type]}</b> {summarizeAction(s)}
            </span>
            <span className="step-tools" onClick={(e) => e.stopPropagation()}>
              <button type="button" className="icon" disabled={i === 0} onClick={() => move(i, -1)} title="위로">
                ▲
              </button>
              <button
                type="button"
                className="icon"
                disabled={i === steps.length - 1}
                onClick={() => move(i, 1)}
                title="아래로"
              >
                ▼
              </button>
              <button type="button" className="icon" onClick={() => remove(i)} title="단계 삭제">
                ✕
              </button>
            </span>
          </div>
          {open === i && (
            <div className="step-body">
              <TypeSelect value={s.type} types={STEP_ACTION_TYPES} onChange={(t) => update(i, emptyAction(t))} />
              <ActionFields action={s} onChange={(a) => update(i, a)} />
            </div>
          )}
        </div>
      ))}
      <div className="chips">
        {STEP_ACTION_TYPES.map((t) => (
          <button key={t} type="button" className="chip" onClick={() => add(t)}>
            + {ACTION_ICONS[t]} {ACTION_LABELS[t]}
          </button>
        ))}
      </div>
    </div>
  );
}

// ---------------- 선택된 버튼 편집 ----------------

interface ButtonEditorProps {
  button: DeckButton;
  onChange: (b: DeckButton) => void;
  onDelete: () => void;
  onTest: () => void;
  onOpenFolder: () => void;
  /** 옮길 수 있는 폴더 목록 (현재 위치 포함) */
  folders: FolderOption[];
  currentPath: Path;
  onMove: (to: Path) => void;
}

export function ButtonEditor({
  button,
  onChange,
  onDelete,
  onTest,
  onOpenFolder,
  folders,
  currentPath,
  onMove,
}: ButtonEditorProps) {
  const set = (patch: Partial<DeckButton>) => onChange({ ...button, ...patch });
  const a = button.action;
  const currentKey = currentPath.join("/");

  function changeType(t: ActionType) {
    if (a.type === "folder" && a.buttons.length > 0) {
      const n = countDeep(a.buttons);
      if (!window.confirm(`폴더 안의 버튼 ${n}개가 함께 사라집니다. 계속할까요?`)) return;
    }
    if (t === "multi" && a.type !== "multi" && a.type !== "folder") {
      // 기존 동작을 첫 단계로 옮겨서 이어서 편집할 수 있게
      set({ action: { type: "multi", steps: [button.action] } });
    } else {
      set({ action: emptyAction(t) });
    }
  }

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
        <TypeSelect value={button.action.type} types={BUTTON_ACTION_TYPES} onChange={changeType} />
      </label>

      {a.type === "folder" ? (
        <div className="folder-info">
          <span>
            📂 안에 버튼 {a.buttons.length}개
            {countDeep(a.buttons) > a.buttons.length && ` (하위 폴더 포함 ${countDeep(a.buttons)}개)`}
          </span>
          <button type="button" className="primary" onClick={onOpenFolder}>
            폴더 열어서 편집 →
          </button>
        </div>
      ) : (
        <ActionFields action={a} onChange={(action) => set({ action })} />
      )}

      <label>
        {a.type === "folder" ? "이 폴더를 바로 여는 전역 단축키 (선택)" : "이 버튼의 전역 단축키 (선택)"}
      </label>
      <HotkeyInput mode="global" value={button.hotkey} onChange={(v) => set({ hotkey: v })} placeholder="없음" />

      {folders.length > 1 && (
        <label>
          위치 (다른 폴더로 옮기기)
          <select
            value={currentKey}
            onChange={(e) => {
              const target = folders.find((f) => f.path.join("/") === e.target.value);
              if (target) onMove(target.path);
            }}
          >
            {folders.map((f) => (
              <option key={f.path.join("/")} value={f.path.join("/")}>
                {f.label}
              </option>
            ))}
          </select>
        </label>
      )}

      <div className="row end">
        <button type="button" className="danger" onClick={onDelete}>
          삭제
        </button>
        {a.type !== "folder" && (
          <button type="button" onClick={onTest} title="지금 설정으로 한 번 실행해 봅니다">
            테스트 실행
          </button>
        )}
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
      <p className="hint">
        버튼을 클릭하면 편집하고, 끌어서 순서를 바꿀 수 있어요. 폴더는 더블클릭하면 안으로 들어갑니다.
      </p>
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
