import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ButtonEditor, SettingsEditor } from "./Editor";
import { prettyHotkey } from "./HotkeyInput";
import { Config, DeckButton, newButton } from "./types";
import "./App.css";

export default function App() {
  const [config, setConfig] = useState<Config | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<Config | null>(null);
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

  if (!config) return <div className="loading">불러오는 중…</div>;

  const view = editing && draft ? draft : config;
  const selected = draft?.buttons.find((b) => b.id === selectedId) ?? null;

  async function press(b: DeckButton) {
    if (editing) {
      setSelectedId(b.id === selectedId ? null : b.id);
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

  function updateButton(b: DeckButton) {
    setDraft((d) => d && { ...d, buttons: d.buttons.map((x) => (x.id === b.id ? b : x)) });
  }

  function addButton() {
    const b = newButton();
    setDraft((d) => d && { ...d, buttons: [...d.buttons, b] });
    setSelectedId(b.id);
  }

  function deleteButton(id: string) {
    setDraft((d) => d && { ...d, buttons: d.buttons.filter((x) => x.id !== id) });
    setSelectedId(null);
  }

  function moveButton(fromId: string, toId: string) {
    if (fromId === toId) return;
    setDraft((d) => {
      if (!d) return d;
      const list = [...d.buttons];
      const from = list.findIndex((x) => x.id === fromId);
      const to = list.findIndex((x) => x.id === toId);
      const [item] = list.splice(from, 1);
      list.splice(to, 0, item);
      return { ...d, buttons: list };
    });
  }

  return (
    <div className={"app" + (editing ? " editing" : "")}>
      <header data-tauri-drag-region>
        <span className="title" data-tauri-drag-region>
          {editing ? "✏️ 편집 모드" : "QuickDeck"}
        </span>
        {!editing && (
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
        {view.buttons.map((b) => (
          <button
            key={b.id}
            className={
              "tile" + (pressedId === b.id ? " pressed" : "") + (selectedId === b.id ? " selected" : "")
            }
            style={{ background: b.color }}
            onClick={() => press(b)}
            title={b.hotkey ? `${b.label} (${prettyHotkey(b.hotkey)})` : b.label}
            draggable={editing}
            onDragStart={() => (dragId.current = b.id)}
            onDragOver={(e) => editing && e.preventDefault()}
            onDrop={() => dragId.current && moveButton(dragId.current, b.id)}
          >
            <span className="tile-icon">{b.icon}</span>
            <span className="tile-label">{b.label}</span>
            {b.hotkey && <span className="tile-badge">⌨</span>}
          </button>
        ))}
        {editing && (
          <button className="tile add" onClick={addButton} title="버튼 추가">
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
              onDelete={() => deleteButton(selected.id)}
              onTest={() =>
                invoke("test_action", { action: selected.action }).catch((e) => showToast(String(e)))
              }
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
