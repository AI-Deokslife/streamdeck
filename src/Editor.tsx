import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import HotkeyInput from "./HotkeyInput";
import { APP_PRESETS, AppPreset } from "./presets";
import { FolderOption, Path, countDeep } from "./tree";
import {
  ACTION_ICONS,
  ACTION_LABELS,
  Action,
  ActionType,
  BUTTON_ACTION_TYPES,
  Config,
  DeckButton,
  RunningApp,
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

// ---------------- 실행 중인 프로그램 고르기 ----------------

/** [실행 중인 프로그램에서 선택] 버튼 → 목록에서 고르면 onPick */
function AppPicker({ onPick, label = "실행 중인 프로그램에서 선택" }: { onPick: (a: RunningApp) => void; label?: string }) {
  const [apps, setApps] = useState<RunningApp[] | null>(null);
  if (!apps) {
    return (
      <button
        type="button"
        className="small"
        onClick={() => invoke<RunningApp[]>("list_running_apps").then(setApps).catch(() => setApps([]))}
      >
        {label}
      </button>
    );
  }
  return (
    <select
      autoFocus
      defaultValue=""
      onChange={(e) => {
        const a = apps.find((x) => x.exe === e.target.value);
        if (a) onPick(a);
        setApps(null);
      }}
      onBlur={() => setApps(null)}
    >
      <option value="" disabled>
        {apps.length ? "프로그램을 고르세요" : "열린 창이 없어요"}
      </option>
      {apps.map((a) => (
        <option key={a.exe} value={a.exe}>
          {a.exe} — {a.title.slice(0, 40)}
        </option>
      ))}
    </select>
  );
}

/** 키/텍스트를 보낼 프로그램 */
function TargetAppField({
  value,
  inherited,
  onChange,
}: {
  value: string;
  inherited?: string | null;
  onChange: (v: string) => void;
}) {
  return (
    <label>
      보낼 프로그램 (선택)
      <div className="row">
        <input
          className="grow"
          value={value}
          placeholder={inherited ? `비우면 폴더에 연결된 ${inherited}(으)로` : "비우면 지금 활성화된 창으로"}
          onChange={(e) => onChange(e.target.value)}
        />
        <AppPicker label="선택" onPick={(a) => onChange(a.exe)} />
      </div>
    </label>
  );
}

// ---------------- 동작 하나의 입력 필드 ----------------

function ActionFields({
  action: a,
  onChange,
  inherited,
}: {
  action: Action;
  onChange: (a: Action) => void;
  /** 상위 폴더에 연결된 프로그램 */
  inherited?: string | null;
}) {
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
          <TargetAppField value={a.app} inherited={inherited} onChange={(app) => set({ app })} />
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
          <TargetAppField value={a.app} inherited={inherited} onChange={(app) => set({ app })} />
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
      return <StepList steps={a.steps} inherited={inherited} onChange={(steps) => set({ steps })} />;
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

function StepList({
  steps,
  onChange,
  inherited,
}: {
  steps: Action[];
  onChange: (s: Action[]) => void;
  inherited?: string | null;
}) {
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
              <ActionFields action={s} inherited={inherited} onChange={(a) => update(i, a)} />
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
  /** 상위 폴더에 연결된 프로그램 */
  inherited: string | null;
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
  inherited,
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
        <ActionFields action={a} inherited={inherited} onChange={(action) => set({ action })} />
      )}

      {a.type === "folder" && (
        <FolderAppFields
          apps={a.apps}
          launch={a.launch}
          onChange={(patch) => set({ action: { ...a, ...patch } })}
        />
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

// ---------------- 폴더 ↔ 프로그램 연결 (스마트 프로필) ----------------

function FolderAppFields({
  apps,
  launch,
  onChange,
}: {
  apps: string[];
  launch: string;
  onChange: (p: { apps?: string[]; launch?: string }) => void;
}) {
  const [typed, setTyped] = useState("");
  const withApp = (exe: string) => {
    const name = exe.trim();
    return !name || apps.some((x) => x.toLowerCase() === name.toLowerCase()) ? apps : [...apps, name];
  };
  const addTyped = () => {
    onChange({ apps: withApp(typed) });
    setTyped("");
  };

  return (
    <div className="link-box">
      <label>연결된 프로그램</label>
      <p className="hint">
        이 프로그램이 앞에 오면 폴더가 자동으로 열리고, 다른 프로그램으로 가면 원래 화면으로 돌아가요. 폴더 안의 단축키
        버튼은 이 프로그램 창으로 보내집니다.
      </p>
      <div className="chips">
        {apps.map((exe) => (
          <span key={exe} className="chip on">
            🔗 {exe}
            <button
              type="button"
              className="chip-x"
              title="연결 해제"
              onClick={() => onChange({ apps: apps.filter((x) => x !== exe) })}
            >
              ✕
            </button>
          </span>
        ))}
        {apps.length === 0 && <span className="hint">연결 없음</span>}
      </div>
      <div className="row">
        <input
          className="grow"
          value={typed}
          placeholder="실행 파일 이름 (예: Hwp.exe)"
          onChange={(e) => setTyped(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && addTyped()}
        />
        <button type="button" className="small" onClick={addTyped}>
          추가
        </button>
        <AppPicker
          label="실행 중에서 선택"
          onPick={(a) => onChange({ apps: withApp(a.exe), launch: launch || a.path })}
        />
      </div>
      <label>
        폴더를 누르면 실행할 프로그램 (선택)
        <input
          value={launch}
          placeholder="실행 파일 전체 경로 — 위 '실행 중에서 선택'으로 고르면 자동으로 채워져요"
          onChange={(e) => onChange({ launch: e.target.value })}
        />
      </label>
      <p className="hint">이미 실행 중이면 새로 띄우지 않고 그 창을 앞으로 가져와요.</p>
    </div>
  );
}

// ---------------- 전체 설정 ----------------

interface SettingsProps {
  config: Config;
  onChange: (c: Config) => void;
  /** 프로그램 프리셋을 지금 화면에 폴더로 추가 */
  onAddPreset: (p: AppPreset) => void;
}

export function SettingsEditor({ config, onChange, onAddPreset }: SettingsProps) {
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

      <label>프로그램 프리셋 추가</label>
      <p className="hint">
        고르면 그 프로그램에서 자주 쓰는 명령이 담긴 폴더가 이 화면에 만들어지고, 그 프로그램이 앞에 오면 자동으로 열려요.
      </p>
      <div className="chips">
        {APP_PRESETS.map((p) => (
          <button key={p.id} type="button" className="chip preset" onClick={() => onAddPreset(p)}>
            <span className="preset-icon" style={{ background: p.color }}>
              {p.icon}
            </span>
            {p.name}
          </button>
        ))}
      </div>
    </div>
  );
}
