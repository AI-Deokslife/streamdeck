import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ButtonEditor, SettingsEditor } from "./Editor";
import { prettyHotkey } from "./HotkeyInput";
import { Path, buttonsAt, countDeep, folderLabels, folderOptions, pathToFolder, updateAt } from "./tree";
import { Config, DeckButton, newButton } from "./types";
import "./App.css";

export default function App() {
  const [config, setConfig] = useState<Config | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<Config | null>(null);
  /** 지금 열려 있는 폴더 경로 (빈 배열 = 처음 화면) */
  const [path, setPath] = useState<Path>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pressedId, setPressedId] = useState<string | null>(null);
  const [toast, setToast] = useState<{ text: string; kind: "error" | "info" } | null>(null);
  const dragId = useRef<string | null>(null);
  const toastTimer = useRef<number>(undefined);

  const showToast = useCallback((text: string, kind: "error" | "info" = "error", ms = 4000) => {
    setToast({ text, kind });
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), ms);
  }, []);

  const startEdit = useCallback(async (cfg: Config) => {
    await invoke("set_edit_mode", { on: true });
    setDraft(structuredClone(cfg));
    setSelectedId(null);
    setEditing(true);
  }, []);

  useEffect(() => {
    invoke<Config>("get_config").then(setConfig).catch((e) => showToast(String(e)));
    // 단축키 등록 실패/자동 변경 같은 시작 알림은 오래 보여준다 (클릭하면 닫힘)
    invoke<string[]>("take_startup_notices").then((notices) => {
      if (notices.length) showToast(notices.join("\n"), "error", 12000);
    });
    const unErr = listen<string>("action-error", (e) => showToast(e.payload));
    return () => {
      unErr.then((f) => f());
    };
  }, [showToast]);

  // 트레이 메뉴의 "버튼 편집"
  useEffect(() => {
    const un = listen("open-editor", () => {
      if (config && !editing) startEdit(config);
    });
    return () => {
      un.then((f) => f());
    };
  }, [config, editing, startEdit]);

  // 폴더 전역 단축키 → 그 폴더 열기
  useEffect(() => {
    const un = listen<string>("open-folder", (e) => {
      const tree = (editing && draft ? draft : config)?.buttons ?? [];
      const p = pathToFolder(tree, e.payload);
      if (p) {
        setPath(p);
        setSelectedId(null);
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, [config, draft, editing]);

  const view = editing && draft ? draft : config;
  const current = view ? buttonsAt(view.buttons, path) : null;

  // 폴더가 삭제됐거나 편집 취소로 사라지면 처음 화면으로
  useEffect(() => {
    if (view && current === null) setPath([]);
  }, [view, current]);

  if (!config || !view) return <div className="loading">불러오는 중…</div>;

  const buttons = current ?? view.buttons;
  const selected = editing ? buttons.find((b) => b.id === selectedId) ?? null : null;
  const crumbs = folderLabels(view.buttons, path);

  function openFolder(id: string) {
    setPath((p) => [...p, id]);
    setSelectedId(null);
  }

  function goTo(depth: number) {
    setPath((p) => p.slice(0, depth));
    setSelectedId(null);
  }

  async function press(b: DeckButton) {
    if (editing) {
      setSelectedId(b.id === selectedId ? null : b.id);
      return;
    }
    if (b.action.type === "folder") {
      openFolder(b.id);
      return;
    }
    setPressedId(b.id);
    setTimeout(() => setPressedId(null), 150);
    try {
      await invoke("run_button", { id: b.id });
    } catch (e) {
      showToast(String(e));
    }
  }

  async function finishEdit(save: boolean) {
    if (save && draft) {
      try {
        const warnings = await invoke<string[]>("save_config", { cfg: draft });
        setConfig(draft);
        if (warnings.length) showToast(warnings.join("\n"));
        else showToast("저장했어요", "info");
      } catch (e) {
        showToast(String(e));
        return;
      }
    }
    await invoke("set_edit_mode", { on: false });
    setEditing(false);
    setDraft(null);
    setSelectedId(null);
  }

  /** 현재 폴더의 버튼 목록을 바꾼다 */
  function editHere(fn: (list: DeckButton[]) => DeckButton[]) {
    setDraft((d) => d && { ...d, buttons: updateAt(d.buttons, path, fn) });
  }

  function updateButton(b: DeckButton) {
    editHere((list) => list.map((x) => (x.id === b.id ? b : x)));
  }

  function addButton() {
    const b = newButton();
    editHere((list) => [...list, b]);
    setSelectedId(b.id);
  }

  function deleteButton(b: DeckButton) {
    if (b.action.type === "folder" && b.action.buttons.length > 0) {
      const n = countDeep(b.action.buttons);
      if (!window.confirm(`'${b.label}' 폴더와 안의 버튼 ${n}개를 모두 삭제할까요?`)) return;
    }
    editHere((list) => list.filter((x) => x.id !== b.id));
    setSelectedId(null);
  }

  function moveToFolder(b: DeckButton, to: Path) {
    if (to.join("/") === path.join("/")) return;
    setDraft((d) => {
      if (!d) return d;
      const removed = updateAt(d.buttons, path, (list) => list.filter((x) => x.id !== b.id));
      return { ...d, buttons: updateAt(removed, to, (list) => [...list, b]) };
    });
    setSelectedId(null);
    showToast(`'${b.label}'을(를) 옮겼어요`, "info", 2000);
  }

  function reorder(fromId: string, toId: string) {
    if (fromId === toId) return;
    editHere((list) => {
      const next = [...list];
      const from = next.findIndex((x) => x.id === fromId);
      const to = next.findIndex((x) => x.id === toId);
      if (from < 0 || to < 0) return list;
      const [item] = next.splice(from, 1);
      next.splice(to, 0, item);
      return next;
    });
  }

  return (
    <div className={"app" + (editing ? " editing" : "")}>
      <header data-tauri-drag-region>
        {path.length === 0 ? (
          <span className="title" data-tauri-drag-region>
            {editing ? "✏️ 편집 모드" : "QuickDeck"}
          </span>
        ) : (
          <nav className="crumbs">
            <button className="crumb" onClick={() => goTo(0)} title="처음 화면">
              🏠
            </button>
            {crumbs.map((name, i) => (
              <span key={i} className="crumb-wrap">
                <span className="crumb-sep">›</span>
                <button className="crumb" onClick={() => goTo(i + 1)} disabled={i === crumbs.length - 1}>
                  {name}
                </button>
              </span>
            ))}
          </nav>
        )}
        {!editing && path.length === 0 && (
          <span className="hint-key" data-tauri-drag-region>
            {prettyHotkey(config.toggleHotkey)}
          </span>
        )}
        <div className="actions">
          {editing ? (
            <>
              <button className="small ghost" onClick={() => finishEdit(false)}>
                취소
              </button>
              <button className="small primary" onClick={() => finishEdit(true)}>
                저장
              </button>
            </>
          ) : (
            <>
              <button className="icon" title="버튼 편집" onClick={() => startEdit(config)}>
                ✏️
              </button>
              <button className="icon" title="숨기기 (트레이에 남아 있어요)" onClick={() => invoke("hide_window")}>
                —
              </button>
            </>
          )}
        </div>
      </header>

      <main className="grid" style={{ gridTemplateColumns: `repeat(${view.columns}, 1fr)` }}>
        {path.length > 0 && (
          <button
            className="tile back"
            onClick={() => goTo(path.length - 1)}
            title={editing ? "상위 폴더로 (버튼을 여기로 끌어 놓으면 상위 폴더로 옮겨져요)" : "상위 폴더로"}
            onDragOver={(e) => editing && e.preventDefault()}
            onDrop={() => {
              const b = buttons.find((x) => x.id === dragId.current);
              if (b) moveToFolder(b, path.slice(0, -1));
            }}
          >
            <span className="tile-icon">⬅</span>
            <span className="tile-label">뒤로</span>
          </button>
        )}
        {buttons.map((b) => {
          const isFolder = b.action.type === "folder";
          return (
            <button
              key={b.id}
              className={
                "tile" +
                (isFolder ? " folder" : "") +
                (pressedId === b.id ? " pressed" : "") +
                (selectedId === b.id ? " selected" : "")
              }
              style={{ background: b.color }}
              onClick={() => press(b)}
              onDoubleClick={() => editing && isFolder && openFolder(b.id)}
              title={b.hotkey ? `${b.label} (${prettyHotkey(b.hotkey)})` : b.label}
              draggable={editing}
              onDragStart={() => (dragId.current = b.id)}
              onDragOver={(e) => editing && e.preventDefault()}
              onDrop={() => dragId.current && reorder(dragId.current, b.id)}
            >
              <span className="tile-icon">{b.icon || (isFolder ? "📁" : "")}</span>
              <span className="tile-label">{b.label}</span>
              {(b.hotkey || b.action.type === "multi" || isFolder) && (
                <span className="tile-badge">
                  {isFolder ? "📂" : ""}
                  {b.action.type === "multi" ? "🔗" : ""}
                  {b.hotkey ? "⌨" : ""}
                </span>
              )}
            </button>
          );
        })}
        {editing && (
          <button className="tile add" onClick={addButton} title="이 화면에 버튼 추가">
            <span className="tile-icon">＋</span>
            <span className="tile-label">추가</span>
          </button>
        )}
      </main>

      {editing && draft && (
        <section className="panel">
          {selected ? (
            <ButtonEditor
              key={selected.id}
              button={selected}
              onChange={updateButton}
              onDelete={() => deleteButton(selected)}
              onTest={() =>
                invoke("test_action", { action: selected.action }).catch((e) => showToast(String(e)))
              }
              onOpenFolder={() => openFolder(selected.id)}
              folders={folderOptions(draft.buttons, selected.id)}
              currentPath={path}
              onMove={(to) => moveToFolder(selected, to)}
            />
          ) : (
            <SettingsEditor config={draft} onChange={setDraft} />
          )}
        </section>
      )}

      {toast && (
        <div className={"toast " + toast.kind} onClick={() => setToast(null)} title="클릭해서 닫기">
          {toast.text}
        </div>
      )}
    </div>
  );
}
