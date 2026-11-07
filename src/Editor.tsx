import { useEffect, useRef, useState } from "react";
import { firstImage, imageToDataUrl } from "./image";
import { IconPatch, IconSource, canAutoIcon, fetchIcon, iconSource, sourceKey } from "./icons";
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
  Theme,
  STEP_ACTION_TYPES,
  emptyAction,
  summarizeAction,
} from "./types";

/** iOS 시스템 색상 (blue, green, indigo, orange, pink, purple, red, teal, yellow, mint, gray, graphite) */
const COLORS = [
  "#0a84ff", "#30d158", "#5e5ce6", "#ff9f0a", "#ff375f", "#bf5af2",
  "#ff453a", "#40c8e0", "#ffd60a", "#63e6e2", "#8e8e93", "#1c1c1e",
];

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
  inherited?: string[];
  onChange: (v: string) => void;
}) {
  return (
    <label>
      보낼 프로그램 (선택)
      <div className="row">
        <input
          className="grow"
          value={value}
          placeholder={
            inherited?.length
              ? `비우면 폴더에 연결된 ${inherited.join(" / ")} 중 열려 있는 창으로`
              : "비우면 지금 활성화된 창으로"
          }
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
  inherited?: string[];
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
        <>
          <label>
            입력할 텍스트
            <textarea rows={3} value={a.text} onChange={(e) => set({ text: e.target.value })} />
          </label>
          <label className="check">
            <input type="checkbox" checked={!!a.typing} onChange={(e) => set({ typing: e.target.checked })} />
            한 글자씩 입력하기 (붙여넣기가 막힌 프로그램용)
          </label>
          <p className="hint">
            기본은 붙여넣기 방식이라 한글·긴 글도 깨지지 않아요. 원래 복사해 둔 내용은 입력 후 그대로 돌려놓습니다.
          </p>
          <TargetAppField value={a.app} inherited={inherited} onChange={(app) => set({ app })} />
        </>
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
  inherited?: string[];
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

// ---------------- 버튼 아이콘 / 이미지 ----------------

/** [아이콘 가져오기] 버튼이 쓸 대상: 프로그램/폴더/웹사이트, 그리고 단축키·텍스트 버튼은 보낼 프로그램 */
function manualIconSource(a: Action, inherited: string[]): IconSource | null {
  const auto = iconSource(a);
  if (auto) return auto;
  if (a.type === "keys" || a.type === "text") {
    const t = (a.app || inherited[0] || "").trim();
    return t ? { kind: "program", target: t } : null;
  }
  return null;
}

function IconPicker({
  button,
  onChange,
  source,
  autoBusy,
}: {
  button: DeckButton;
  onChange: (p: Partial<DeckButton>) => void;
  /** 아이콘을 가져올 수 있는 프로그램/사이트 */
  source: IconSource | null;
  /** 자동 아이콘을 가져오는 중 */
  autoBusy: boolean;
}) {
  const fileRef = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [capturing, setCapturing] = useState(false);
  const [error, setError] = useState("");

  async function applyImage(file: Blob | null) {
    if (!file) return;
    setError("");
    try {
      onChange({ image: await imageToDataUrl(file), imageAuto: false });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function useSourceIcon() {
    if (!source) return;
    setBusy(true);
    setError("");
    try {
      onChange(await fetchIcon(source));
    } catch (e) {
      setError(String(e));
    }
    setBusy(false);
  }

  /** Windows 캡처 도구로 화면 일부를 잘라 버튼 이미지로 (캡처하는 동안 QuickDeck 창은 잠깐 숨음) */
  async function captureScreen() {
    setCapturing(true);
    setError("");
    try {
      const dataUrl = await invoke<string>("capture_screen");
      const blob = await (await fetch(dataUrl)).blob();
      // 캡처는 보통 그림이므로 버튼 전체를 채우는 쪽이 자연스럽다
      onChange({ image: await imageToDataUrl(blob), imageFull: true, imageAuto: false });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
    setCapturing(false);
  }

  return (
    <div className="icon-picker">
      <label>아이콘</label>
      <div className="row">
        <div
          className="icon-preview"
          style={{ background: button.color }}
          tabIndex={0}
          title="이미지를 끌어다 놓거나, 클릭 후 Ctrl+V로 붙여넣기"
          onPaste={(e) => {
            const f = firstImage(e.clipboardData);
            if (f) {
              e.preventDefault();
              applyImage(f);
            }
          }}
          onDragOver={(e) => e.preventDefault()}
          onDrop={(e) => {
            e.preventDefault();
            applyImage(firstImage(e.dataTransfer));
          }}
        >
          {button.image ? (
            <img className={button.imageFull ? "full" : ""} src={button.image} alt="" />
          ) : (
            <span>{button.icon || "?"}</span>
          )}
        </div>
        <div className="icon-controls">
          <div className="row">
            <input
              className="emoji-input"
              value={button.icon}
              placeholder="이모지"
              title="이미지가 없을 때 보이는 이모지/글자"
              onChange={(e) => onChange({ icon: e.target.value })}
            />
            <button type="button" className="small" onClick={() => fileRef.current?.click()}>
              🖼 이미지 선택
            </button>
            {capturing ? (
              <button type="button" className="small primary" onClick={() => invoke("cancel_capture")}>
                캡처 취소
              </button>
            ) : (
              <button type="button" className="small" onClick={captureScreen} title="화면에서 영역을 드래그해서 잘라 넣기">
                ✂ 화면 캡처
              </button>
            )}
            {source && (
              <button
                type="button"
                className="small"
                disabled={busy || autoBusy}
                onClick={useSourceIcon}
                title={source.kind === "program" ? source.target : source.url}
              >
                {busy || autoBusy ? "가져오는 중…" : source.kind === "site" ? "🌐 사이트 아이콘" : "📦 프로그램 아이콘"}
              </button>
            )}
          </div>
          {button.image ? (
            <div className="row">
              <label className="check">
                <input
                  type="checkbox"
                  checked={!!button.imageFull}
                  onChange={(e) => onChange({ imageFull: e.target.checked })}
                />
                버튼 전체에 꽉 채우기
              </label>
              <button type="button" className="small ghost" onClick={() => onChange({ image: "", imageFull: false, imageAuto: false })}>
                이미지 빼기
              </button>
            </div>
          ) : (
            <span className="hint">왼쪽 칸에 이미지를 끌어 놓거나, 클릭하고 Ctrl+V로 붙여넣어도 돼요</span>
          )}
          {capturing && (
            <span className="keycap-hint">캡처할 영역을 마우스로 드래그하세요. Esc를 누르면 캡처 도구가 닫혀요.</span>
          )}
          {error && <span className="keycap-hint warn">{error}</span>}
        </div>
      </div>
      <input
        ref={fileRef}
        type="file"
        accept="image/*"
        hidden
        onChange={(e) => {
          applyImage(e.target.files?.[0] ?? null);
          e.target.value = "";
        }}
      />
    </div>
  );
}

// ---------------- 선택된 버튼 편집 ----------------

interface ButtonEditorProps {
  button: DeckButton;
  onChange: (b: DeckButton) => void;
  onDelete: () => void;
  onDuplicate: () => void;
  onCopy: () => void;
  onTest: () => void;
  onOpenFolder: () => void;
  /** 옮길 수 있는 폴더 목록 (현재 위치 포함) */
  folders: FolderOption[];
  currentPath: Path;
  onMove: (to: Path) => void;
  /** 상위 폴더에 연결된 프로그램 */
  inherited: string[];
  /** 자동으로 가져온 아이콘을 최신 편집 상태에 반영 */
  onAutoIcon: (id: string, patch: IconPatch) => void;
}

/** 실행할 프로그램·폴더 연결·웹 주소를 정하면 (입력이 멈추고 잠시 뒤) 아이콘을 자동으로 가져온다.
 *  직접 넣은 이미지는 건드리지 않고, 처음 열 때 이미 이미지가 있으면 다시 가져오지 않는다. */
function useAutoIcon(button: DeckButton, onAutoIcon: (id: string, patch: IconPatch) => void) {
  const source = iconSource(button.action);
  const key = sourceKey(source);
  const [busy, setBusy] = useState(false);
  const first = useRef(true);
  const latest = useRef(button);
  latest.current = button;

  useEffect(() => {
    const isFirst = first.current;
    first.current = false;
    if (!source || !canAutoIcon(latest.current)) return;
    if (isFirst && latest.current.image) return;
    let cancelled = false;
    const timer = window.setTimeout(async () => {
      setBusy(true);
      try {
        const patch = await fetchIcon(source);
        if (!cancelled) onAutoIcon(latest.current.id, patch);
      } catch {
        // 못 찾으면 조용히 넘어간다 (아이콘 칸의 버튼으로 다시 시도 가능)
      } finally {
        if (!cancelled) setBusy(false);
      }
    }, 700);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      setBusy(false);
    };
    // 대상(key)이 바뀔 때만
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return busy;
}

export function ButtonEditor({
  button,
  onChange,
  onDelete,
  onDuplicate,
  onCopy,
  onTest,
  onOpenFolder,
  folders,
  currentPath,
  onMove,
  inherited,
  onAutoIcon,
}: ButtonEditorProps) {
  const set = (patch: Partial<DeckButton>) => onChange({ ...button, ...patch });
  const a = button.action;
  const autoBusy = useAutoIcon(button, onAutoIcon);
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
      <label>
        이름
        <input value={button.label} onChange={(e) => set({ label: e.target.value })} />
      </label>

      <IconPicker button={button} onChange={set} source={manualIconSource(a, inherited)} autoBusy={autoBusy} />

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
        <span className="grow" />
        <button type="button" onClick={onCopy} title="복사해서 다른 폴더에 붙여넣기 (Ctrl+C)">
          복사
        </button>
        <button type="button" onClick={onDuplicate} title="바로 옆에 복사본 만들기 (Ctrl+D)">
          복제
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
  const [finding, setFinding] = useState(false);
  const withApp =(exe: string) => {
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
        폴더를 누르면 실행할 프로그램
        <div className="row">
          <input
            className="grow"
            value={launch}
            placeholder="비워 두면 연결된 프로그램의 설치 위치를 자동으로 찾아요"
            onChange={(e) => onChange({ launch: e.target.value })}
          />
          <button
            type="button"
            className="small"
            disabled={!apps.length || finding}
            onClick={async () => {
              setFinding(true);
              for (const exe of apps) {
                const found = await invoke<string | null>("resolve_app_path", { exe }).catch(() => null);
                if (found) {
                  onChange({ launch: found });
                  setFinding(false);
                  return;
                }
              }
              setFinding(false);
              window.alert(`${apps.join(", ")} 설치 위치를 찾지 못했어요. 경로를 직접 입력해 주세요.`);
            }}
          >
            {finding ? "찾는 중…" : "자동으로 찾기"}
          </button>
        </div>
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
  /** 알림 띄우기 */
  onNotice: (text: string, kind?: "error" | "info") => void;
  /** 모든 버튼에 프로그램·사이트 아이콘 넣기 */
  onApplyAllIcons: () => Promise<void>;
}

export function SettingsEditor({ config, onChange, onAddPreset, onNotice, onApplyAllIcons }: SettingsProps) {
  const [applying, setApplying] = useState(false);
  async function exportConfig() {
    try {
      const path = await invoke<string | null>("export_config", { cfg: config });
      if (path) onNotice(`내보냈어요: ${path}`, "info");
    } catch (e) {
      onNotice(String(e));
    }
  }

  async function importConfig() {
    try {
      const cfg = await invoke<Config | null>("import_config");
      if (!cfg) return;
      onChange(cfg);
      onNotice("설정을 불러왔어요. 확인하고 위의 '저장'을 누르면 적용돼요 (취소하면 원래대로)", "info");
    } catch (e) {
      onNotice(String(e));
    }
  }

  return (
    <div className="editor">
      <p className="hint">
        버튼을 클릭하면 편집하고, 끌어서 순서를 바꿀 수 있어요. 폴더는 더블클릭하면 안으로 들어갑니다.
      </p>
      <label>화면 테마</label>
      <div className="segmented" role="radiogroup" aria-label="화면 테마">
        {(
          [
            ["auto", "자동"],
            ["light", "밝게"],
            ["dark", "어둡게"],
          ] as [Theme, string][]
        ).map(([value, name]) => (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={(config.theme ?? "auto") === value}
            className={(config.theme ?? "auto") === value ? "on" : ""}
            onClick={() => onChange({ ...config, theme: value })}
          >
            {name}
          </button>
        ))}
      </div>
      <p className="hint">자동은 Windows의 밝게/어둡게 설정을 따라가요.</p>

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

      <label>아이콘</label>
      <p className="hint">
        프로그램 실행·폴더 버튼은 프로그램 아이콘을, 웹사이트 버튼은 사이트 아이콘을 넣어요. 직접 넣은 이미지는 그대로 둡니다.
      </p>
      <div className="row">
        <button
          type="button"
          className="small"
          disabled={applying}
          onClick={async () => {
            setApplying(true);
            await onApplyAllIcons();
            setApplying(false);
          }}
        >
          {applying ? "가져오는 중…" : "✨ 모든 버튼에 아이콘 넣기"}
        </button>
      </div>

      <label>설정 백업</label>
      <p className="hint">저장할 때마다 이전 설정이 자동으로 백업돼요 (최근 30개). 다른 PC로 옮길 때는 내보내기를 쓰세요.</p>
      <div className="row">
        <button type="button" className="small" onClick={exportConfig}>
          내보내기
        </button>
        <button type="button" className="small" onClick={importConfig}>
          가져오기 / 백업에서 복원
        </button>
        <button type="button" className="small ghost" onClick={() => invoke("open_backup_folder")}>
          백업 폴더 열기
        </button>
      </div>
    </div>
  );
}
